use bollard::models::PortBinding;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Состояние задачи — enum с дополнительными данными для каждого варианта
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    /// Ожидает назначения на воркер
    Pending,
    /// Назначена, но еще не запущена
    Scheduled,
    /// Выполняется, с временем старта
    Running { started_at: DateTime<Utc> },
    /// Успешно завершена
    Completed { finished_at: DateTime<Utc> },
    /// Завершена с ошибкой
    Failed {
        error: String,
        finished_at: DateTime<Utc>,
    },
}

impl TaskState {
    /// Проверка, активна ли задача (выполняется или ожидает)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskState::Pending | TaskState::Scheduled | TaskState::Running { .. }
        )
    }

    /// Проверка, завершена ли задача
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskState::Completed { .. } | TaskState::Failed { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub image: String,
    pub state: TaskState,

    // Ресурсы
    pub cpu: f64,    // доли ядра
    pub memory: i64, // байты
    pub disk: i64,   // байты

    // Docker-specific
    pub container_id: Option<String>,
    pub exposed_ports: Vec<u16>, // Option<Vec<String>> - порты, которые открывает контейнер
    pub port_bindings: Vec<PortBinding>, // маппинг на хост (из bollard)
    pub restart_policy: RestartPolicy,
    pub env_vars: Vec<String>,

    // Метки для фильтрации
    pub labels: std::collections::HashMap<String, String>,
}

impl Task {
    pub fn new(
        id: Uuid,
        name: String,
        state: TaskState,
        image: String,
        memory: i64,
        disk: i64,
    ) -> Self {
        Task {
            id,
            name,
            state,
            image,
            memory,
            disk,
            cpu: 0.0, // default CPU
            container_id: None,
            exposed_ports: Vec::new(),
            port_bindings: Vec::new(),
            restart_policy: RestartPolicy::default(),
            env_vars: Vec::new(),
            labels: std::collections::HashMap::new(),
        }
    }
}

/// Политика перезапуска — тоже enum
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RestartPolicy {
    #[default]
    No,
    Always,
    OnFailure {
        max_retries: Option<u32>,
    },
    UnlessStopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: Uuid,
    pub state: TaskState,
    pub timestamp: DateTime<Utc>,
    pub task: Task,
}

impl TaskEvent {
    pub fn new(id: Uuid, state: TaskState, timestamp: DateTime<Utc>, task: Task) -> Self {
        TaskEvent {
            id,
            state,
            timestamp,
            task,
        }
    }
}
