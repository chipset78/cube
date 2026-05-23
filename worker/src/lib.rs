use anyhow::anyhow;
use log::{error, info};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use task::{ContainerConfig, DockerContainer, DockerResult, Task, TaskState};

/// Состояние воркера (для мониторинга)
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerStatus {
    Idle,
    Busy,
    Draining,
    Shutdown,
}

/// Статистика воркера
#[derive(Debug, Clone)]
pub struct WorkerStats {
    pub name: String,
    pub queue_size: usize,
    pub tasks_in_db: usize,
    pub task_count: i32,
    pub status: WorkerStatus,
}

/// Worker
#[derive(Debug)]
pub struct Worker {
    pub name: String,
    pub queue: VecDeque<Task>,
    pub db: HashMap<Uuid, Arc<Mutex<Task>>>,
    pub task_count: i32,
    pub status: WorkerStatus,
}

impl Worker {
    /// Создание нового worker
    pub fn new(name: String) -> Self {
        Worker {
            name,
            queue: VecDeque::new(),
            db: HashMap::new(),
            task_count: 0,
            status: WorkerStatus::Idle,
        }
    }

    /// Выводит статистику
    pub async fn collect_stats(&self) {
        println!("I will collect stats");
    }

    /// Выполняет задачу из очереди
    pub async fn run_task(&mut self) -> Option<DockerResult> {
        // Извлекаем задачу из очереди
        let task_queued = match self.queue.pop_front() {
            Some(t) => t,
            None => {
                info!("No tasks in the queue");
                return Some(DockerResult::success("noop", ""));
            }
        };
        let task_id = task_queued.id;

        // Получаем или создаем задачу в БД
        let task_arc = if let Some(existing) = self.db.get(&task_id).cloned() {
            existing
        } else {
            let new_task_arc = Arc::new(Mutex::new(task_queued.clone()));
            self.db.insert(task_id, new_task_arc.clone());
            new_task_arc
        };

        // Проверяем валидность перехода состояния
        {
            let task_guard = task_arc.lock().await;
            let src_state = Some(&task_guard.state);
            let dst_state = &task_queued.state;

            if !Self::valid_state_transition(src_state, dst_state) {
                // Исправление #1: убираем unnecessary_literal_unwrap
                let src_display = match src_state {
                    Some(state) => format!("{:?}", state),
                    None => format!("{:?}", TaskState::Pending),
                };
                let err_msg = format!("Invalid transition from {} to {:?}", src_display, dst_state);
                error!("{}", err_msg);
                return Some(DockerResult::failure("transition", anyhow!(err_msg)));
            }
        }

        // Выполняем соответствующее действие в зависимости от состояния
        let result = match task_queued.state {
            TaskState::Scheduled => self.start_task(task_arc.clone()).await,
            TaskState::Completed { .. } => self.stop_task(task_arc.clone()).await,
            _ => {
                let err = anyhow!(
                    "We should not get here: invalid state {:?}",
                    task_queued.state
                );
                error!("{}", err);
                DockerResult::failure("invalid_state", err)
            }
        };

        Some(result)
    }

    /// Запускает задачу
    pub async fn start_task(&mut self, task_arc: Arc<Mutex<Task>>) -> DockerResult {
        let start_time = chrono::Utc::now();

        // Получаем блокировку для чтения данных задачи
        let task_guard = task_arc.lock().await;
        let task_id = task_guard.id;
        let image = task_guard.image.clone();
        let env_vars = task_guard.env_vars.clone();
        let cpu = task_guard.cpu;
        let memory = task_guard.memory;
        drop(task_guard); // Освобождаем блокировку перед длительной операцией

        // Создаем конфигурацию для Docker
        let config = ContainerConfig::new(image, format!("task-{}", task_id))
            .with_env(env_vars)
            .with_resources(cpu, memory);

        // Создаем Docker клиент и запускаем контейнер
        let mut docker = match DockerContainer::new(config).await {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to create Docker client for task {}: {}", task_id, e);

                // Обновляем состояние задачи в БД
                let mut task_guard = task_arc.lock().await;
                task_guard.state = TaskState::Failed {
                    error: e.to_string(),
                    finished_at: chrono::Utc::now(),
                };
                return DockerResult::failure("docker_init", e);
            }
        };

        let result = docker.run().await;

        if let Some(ref error_msg) = result.error {
            error!("Err running task {}: {:?}", task_id, error_msg);

            // Обновляем состояние задачи в БД
            let mut task_guard = task_arc.lock().await;
            task_guard.state = TaskState::Failed {
                error: error_msg.to_string(),
                finished_at: chrono::Utc::now(),
            };
            return result;
        }

        // Обновляем задачу с полученным ContainerID и новым состоянием
        let mut task_guard = task_arc.lock().await;
        task_guard.container_id = Some(result.container_id.clone());
        task_guard.state = TaskState::Running {
            started_at: start_time,
        };
        drop(task_guard);

        info!(
            "Started task {} with container {}",
            task_id, result.container_id
        );
        result
    }

    /// Останавливает задачу
    pub async fn stop_task(&mut self, task_arc: Arc<Mutex<Task>>) -> DockerResult {
        // Получаем container_id под блокировкой
        let (task_id, container_id_opt, image) = {
            let task_guard = task_arc.lock().await;
            (
                task_guard.id,
                task_guard.container_id.clone(),
                task_guard.image.clone(),
            )
        };

        let container_id = match container_id_opt {
            Some(id) => id,
            None => {
                let err = anyhow!("Task {} has no container ID", task_id);
                error!("{}", err);
                return DockerResult::failure("no_container", err);
            }
        };

        // Создаем конфигурацию для Docker
        let config = ContainerConfig::new(image, format!("task-{}", task_id));

        // Создаем Docker клиент и останавливаем контейнер
        let docker = match DockerContainer::new(config).await {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to create Docker client for task {}: {}", task_id, e);
                return DockerResult::failure("docker_init", e);
            }
        };

        let result = docker.stop(&container_id).await;

        if result.error.is_some() {
            error!(
                "Error stopping container {}: {:?}",
                container_id, result.error
            );
        }

        // Обновляем состояние задачи в БД
        let mut task_guard = task_arc.lock().await;
        task_guard.state = TaskState::Completed {
            finished_at: chrono::Utc::now(),
        };
        drop(task_guard);

        info!(
            "Stopped and removed container {} for task {}",
            container_id, task_id
        );
        result
    }

    /// Добавляет задачу в очередь
    pub fn add_task(&mut self, task: Task) {
        self.queue.push_back(task);
        self.task_count += 1;
        info!("Added task to queue. Total tasks: {}", self.task_count);
    }

    /// Получает задачу из БД по ID
    pub async fn get_task(&self, task_id: &Uuid) -> Option<Task> {
        self.db
            .get(task_id)
            .cloned()
            .and_then(|arc| arc.try_lock().ok().map(|guard| guard.clone()))
    }

    /// Обновляет состояние задачи
    pub async fn update_task_state<F>(&mut self, task_id: Uuid, updater: F) -> bool
    where
        F: FnOnce(&mut Task),
    {
        if let Some(task_arc) = self.db.get(&task_id) {
            let mut task_guard = task_arc.lock().await;
            updater(&mut task_guard);
            true
        } else {
            false
        }
    }

    /// Получение статистики воркера
    pub fn get_stats(&self) -> WorkerStats {
        WorkerStats {
            name: self.name.clone(),
            queue_size: self.queue.len(),
            tasks_in_db: self.db.len(),
            task_count: self.task_count,
            status: self.status.clone(),
        }
    }

    /// Проверка валидного перехода состояния
    fn valid_state_transition(src_state: Option<&TaskState>, dst_state: &TaskState) -> bool {
        use TaskState::*;

        match (src_state, dst_state) {
            // Новая задача
            (None, Pending) => true,

            // Из Pending
            (Some(Pending), Scheduled) => true,
            (Some(Pending), Failed { .. }) => true,

            // Из Scheduled
            (Some(Scheduled), Running { .. }) => true,
            (Some(Scheduled), Failed { .. }) => true,

            // Из Running
            (Some(Running { .. }), Completed { .. }) => true,
            (Some(Running { .. }), Failed { .. }) => true,

            // Все остальные переходы запрещены
            _ => false,
        }
    }
}
