use chrono::Utc;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

use manager::Manager;
use node::Node;
use task::{Task, TaskEvent, TaskState};
use worker::Worker;

use task::docker::{ContainerConfig, DockerContainer, stop_container};

/// CLI для управления Docker контейнерами и оркестрацией
#[derive(Parser)]
#[command(name = "docker-cli")]
#[command(about = "CLI для управления контейнерами", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Создать и запустить новый контейнер
    Run {
        /// Имя образа
        #[arg(short, long, default_value = "postgres:16")]
        image: String,

        /// Имя контейнера
        #[arg(short, long, default_value = "test-container")]
        name: String,

        /// Переменные окружения (формат KEY=VALUE)
        #[arg(short, long, value_parser = parse_env_var)]
        env: Vec<(String, String)>,

        /// Команда для выполнения
        #[arg(long, num_args = 1..)]
        command: Option<Vec<String>>,

        /// CPU (в ядрах)
        #[arg(short = 'c', long, default_value = "0.5")]
        cpu: f64,

        /// Память (в MB)
        #[arg(short = 'm', long, default_value = "512")]
        memory_mb: i64,

        /// Политика перезапуска (no, always, on-failure, unless-stopped)
        #[arg(short = 'r', long, default_value = "no")]
        restart_policy: String,

        /// Пробросить порт (можно использовать несколько раз)
        #[arg(short = 'p', long, action = clap::ArgAction::Append)]
        ports: Vec<String>,
    },

    /// Остановить и удалить контейнер
    Stop {
        /// ID контейнера
        #[arg(short, long)]
        container_id: String,
    },

    /// Список запущенных контейнеров (используя docker ps)
    Ps {
        /// Показать все контейнеры
        #[arg(short, long)]
        all: bool,
    },

    /// Создать новую задачу
    CreateTask {
        /// Имя задачи
        #[arg(short, long)]
        name: String,

        /// Имя образа
        #[arg(short, long)]
        image: String,

        /// Лимит памяти (MB)
        #[arg(short, long, default_value = "1024")]
        memory: i64,

        /// Лимит диска (GB)
        #[arg(short, long, default_value = "1")]
        disk: i64,
    },

    /// Показать информацию о задаче
    ShowTask {
        /// ID задачи
        #[arg(short, long)]
        task_id: Uuid,
    },

    /// Управление воркером
    Worker {
        #[command(subcommand)]
        action: WorkerAction,
    },

    /// Управление менеджером
    Manager {
        #[command(subcommand)]
        action: ManagerAction,
    },

    /// Управление нодой
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },

    /// Полный тестовый сценарий
    Test,
}

#[derive(Subcommand)]
enum WorkerAction {
    /// Создать нового воркера
    Create {
        /// Имя воркера
        #[arg(short, long)]
        name: String,
    },

    /// Добавить задачу в воркер
    AddTask {
        /// ID задачи
        #[arg(short, long)]
        task_id: Uuid,
    },

    /// Запустить задачу на воркере
    RunTask {
        /// ID задачи
        #[arg(short, long)]
        task_id: Uuid,
    },

    /// Остановить задачу на воркере
    StopTask {
        /// ID задачи
        #[arg(short, long)]
        task_id: Uuid,
    },

    /// Показать статистику воркера
    Stats {
        /// Имя воркера
        #[arg(short, long)]
        name: String,
    },

    /// Показать информацию о воркере
    Info {
        /// Имя воркера
        #[arg(short, long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum ManagerAction {
    /// Создать менеджера
    Create,

    /// Добавить воркера в менеджер
    AddWorker {
        /// Имя воркера
        #[arg(short, long)]
        worker_name: String,
    },

    /// Выбрать воркера для задачи
    SelectWorker,

    /// Обновить статус задач
    UpdateTasks,

    /// Отправить работу воркеру
    SendWork,

    /// Показать информацию о менеджере
    Info,
}

#[derive(Subcommand)]
enum NodeAction {
    /// Создать новую ноду
    Create {
        /// Имя ноды
        #[arg(short, long)]
        name: String,

        /// IP адрес
        #[arg(short, long)]
        ip: String,

        /// Количество ядер
        #[arg(short, long, default_value = "4")]
        cores: i32,

        /// Память (MB)
        #[arg(short, long, default_value = "1024")]
        memory: i32,

        /// Диск (GB)
        #[arg(short, long, default_value = "25")]
        disk: i32,

        /// Тип ноды
        #[arg(short = 't', long, default_value = "worker")]
        node_type: String,
    },

    /// Показать информацию о ноде
    Info {
        /// Имя ноды
        #[arg(short, long)]
        name: String,
    },
}

// Парсер для переменных окружения
fn parse_env_var(s: &str) -> Result<(String, String), String> {
    if let Some(pos) = s.find('=') {
        let key = s[..pos].to_string();
        let value = s[pos + 1..].to_string();
        Ok((key, value))
    } else {
        Err(format!(
            "Переменная окружения должна быть в формате KEY=VALUE: {}",
            s
        ))
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            image,
            name,
            env,
            command,
            cpu,
            memory_mb,
            restart_policy,
            ports,
        } => {
            run_container_cmd(RunContainerParams {
                image,
                name,
                env,
                command,
                cpu,
                memory_mb,
                restart_policy,
                ports,
            })
            .await;
        }

        Commands::Stop { container_id } => {
            stop_container_cmd(&container_id).await;
        }

        Commands::Ps { all } => {
            list_containers_cmd(all).await;
        }

        Commands::CreateTask {
            name,
            image,
            memory,
            disk,
        } => {
            create_task_cmd(&name, &image, memory, disk).await;
        }

        Commands::ShowTask { task_id } => {
            show_task_cmd(task_id).await;
        }

        Commands::Worker { action } => {
            handle_worker_action(action).await;
        }

        Commands::Manager { action } => {
            handle_manager_action(action).await;
        }

        Commands::Node { action } => {
            handle_node_action(action).await;
        }

        Commands::Test => {
            run_test_scenario().await;
        }
    }
}

// Определяем структуру для параметров запуска контейнера
#[derive(Debug)]
struct RunContainerParams {
    image: String,
    name: String,
    env: Vec<(String, String)>,
    command: Option<Vec<String>>,
    cpu: f64,
    memory_mb: i64,
    restart_policy: String,
    ports: Vec<String>,
}

// Обновленная функция запуска контейнера с использованием новой конфигурации
async fn run_container_cmd(params: RunContainerParams) {
    println!(" * Создание контейнера...");
    println!("   Образ: {}", params.image);
    println!("   Имя: {}", params.name);
    println!("   CPU: {} ядер", params.cpu);
    println!("   Память: {} MB", params.memory_mb);
    println!("   Политика перезапуска: {}", params.restart_policy);

    // Создаем конфигурацию
    let mut config = ContainerConfig::new(params.image, params.name)
        .with_resources(params.cpu, params.memory_mb * 1024 * 1024) // Конвертируем MB в байты
        .with_restart_policy(params.restart_policy);

    // Добавляем переменные окружения
    if !params.env.is_empty() {
        let env_vars: Vec<String> = params
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        config = config.with_env(env_vars);
    }

    // Добавляем команду
    if let Some(cmd_args) = params.command
        && !cmd_args.is_empty()
    {
        config = config.with_cmd(cmd_args);
    }

    // Добавляем порты
    for port in params.ports {
        config = config.with_exposed_port(&port);
    }

    // Создаем и запускаем контейнер
    match DockerContainer::new(config).await {
        Ok(mut docker) => {
            let result = docker.run().await;

            if result.is_success() {
                println!("\n * Контейнер успешно создан и запущен!");
                println!("   ID контейнера: {}", result.container_id);
                println!("   Действие: {}", result.action);
                println!("   Результат: {}", result.result);
            } else {
                println!("\nX Ошибка при создании контейнера!");
                if let Some(error) = result.error {
                    println!("   Ошибка: {}", error);
                }
            }
        }
        Err(e) => {
            eprintln!(" X Ошибка при создании Docker клиента: {}", e);
        }
    }
}

// Обновленная функция остановки контейнера
async fn stop_container_cmd(container_id: &str) {
    println!(" * Остановка и удаление контейнера: {}", container_id);

    // Создаем временную конфигурацию для Docker клиента
    let temp_config = ContainerConfig::new("temp".to_string(), "temp".to_string());

    match DockerContainer::new(temp_config).await {
        Ok(docker) => match stop_container(&docker, container_id).await {
            Some(result) => {
                if result.is_success() {
                    println!(" * Контейнер успешно остановлен и удален!");
                    println!("   ID контейнера: {}", result.container_id);
                    println!("   Действие: {}", result.action);
                    println!("   Результат: {}", result.result);
                } else {
                    println!(" X Ошибка при остановке контейнера!");
                    if let Some(error) = result.error {
                        println!("   Ошибка: {}", error);
                    }
                }
            }
            None => {
                println!(" X Не удалось остановить контейнер (результат None)");
            }
        },
        Err(e) => {
            eprintln!(" X Ошибка при создании Docker клиента: {}", e);
        }
    }
}

// Функция для отображения списка контейнеров
async fn list_containers_cmd(all: bool) {
    let temp_config = ContainerConfig::new("temp".to_string(), "temp".to_string());
    
    match DockerContainer::new(temp_config).await {
        Ok(docker) => {
            match docker.list_containers(all).await {
                Ok(containers) => {
                    if containers.is_empty() {
                        println!("Нет контейнеров");
                        return;
                    }

                    println!("Список контейнеров:");
                    println!("{:<15} {:<30} {:<20} {:<15}", "ID", "Имя", "Статус", "Образ");
                    println!("{}", "-".repeat(80));

                    for container in containers {
                        println!(
                            "{:<15} {:<30} {:<20} {:<15}",
                            container.short_id, container.name, container.status, container.image
                        );
                    }
                }
                Err(e) => {
                    eprintln!(" X {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!(" X Ошибка подключения к Docker: {}", e);
        }
    }
}

async fn create_task_cmd(name: &str, image: &str, memory: i64, disk: i64) {
    let task = Task::new(
        Uuid::new_v4(),
        name.to_string(),
        TaskState::Pending,
        image.to_string(),
        memory,
        disk,
    );

    let task_event = TaskEvent::new(Uuid::new_v4(), TaskState::Pending, Utc::now(), task.clone());

    println!(" * Задача создана:");
    println!("   ID: {}", task.id);
    println!("   Имя: {}", task.name);
    println!("   Статус: {:?}", task.state);
    println!("   Образ: {}", task.image);
    println!("   Память: {} MB", task.memory);
    println!("   Диск: {} GB", task.disk);
    println!("\n * Событие задачи:");
    println!("   ID события: {}", task_event.id);
    println!("   Статус: {:?}", task_event.state);
    println!("   Время: {}", task_event.timestamp);
}

async fn show_task_cmd(task_id: Uuid) {
    println!("   Информация о задаче: {}", task_id);
    println!("   (Функциональность требует реализации метода get_task из базы данных)");
}

async fn handle_worker_action(action: WorkerAction) {
    match action {
        WorkerAction::Create { name } => {
            let worker = Worker::new(name.clone());
            println!(" * Воркер создан:");
            println!("   Имя: {}", worker.name);
            println!("   Статус: {:?}", worker.status);
            println!("   Задачи в очереди: {}", worker.queue.len());
        }

        WorkerAction::AddTask { task_id } => {
            let mut worker = Worker::new("default-worker".to_string());
            let task = Task::new(
                task_id,
                "Task".to_string(),
                TaskState::Pending,
                "image".to_string(),
                512,
                10,
            );
            worker.add_task(task.clone());
            println!(" * Задача {} добавлена в очередь воркера", task_id);
            println!("   Задач в очереди: {}", worker.queue.len());
        }

        WorkerAction::RunTask { task_id } => {
            println!(" * Запуск задачи {} на воркере", task_id);
            let task = Task::new(
                task_id,
                "Task".to_string(),
                TaskState::Pending,
                "image".to_string(),
                512,
                10,
            );
            let task_arc = Arc::new(Mutex::new(task));
            let mut worker = Worker::new("default-worker".to_string());

            worker.run_task().await;
            worker.start_task(task_arc.clone()).await;

            println!(" * Задача {} запущена", task_id);
        }

        WorkerAction::StopTask { task_id } => {
            println!(" * Остановка задачи {}", task_id);
            let task = Task::new(
                task_id,
                "Task".to_string(),
                TaskState::Completed {
                    finished_at: Utc::now(),
                },
                "image".to_string(),
                512,
                10,
            );
            let task_arc = Arc::new(Mutex::new(task));
            let mut worker = Worker::new("default-worker".to_string());

            worker.stop_task(task_arc).await;
            println!(" * Задача {} остановлена", task_id);
        }

        WorkerAction::Stats { name } => {
            let worker = Worker::new(name);
            worker.collect_stats().await;
            println!(" * Статистика воркера:");
            println!("   Имя: {}", worker.name);
            println!("   Статус: {:?}", worker.status);
            println!("   Задач в очереди: {}", worker.queue.len());
        }

        WorkerAction::Info { name } => {
            let worker = Worker::new(name);
            println!(" * Информация о воркере:");
            println!("   Имя: {}", worker.name);
            println!("   Статус: {:?}", worker.status);
        }
    }
}

async fn handle_manager_action(action: ManagerAction) {
    match action {
        ManagerAction::Create => {
            let manager = Manager::new();
            println!(" * Менеджер создан");
            println!("   Воркеров в пуле: {}", manager.workers.len());
        }

        ManagerAction::AddWorker { worker_name } => {
            let mut manager = Manager::new();
            manager.workers.push(worker_name.clone());
            println!(" * Воркер {} добавлен в менеджер", worker_name);
            println!("   Всего воркеров: {}", manager.workers.len());
        }

        ManagerAction::SelectWorker => {
            let mut manager = Manager::new();
            manager.workers.push("worker-1".to_string());
            manager.workers.push("worker-2".to_string());
            manager.select_worker();
            println!("Воркер выбран");
        }

        ManagerAction::UpdateTasks => {
            let manager = Manager::new();
            manager.update_tasks();
            println!(" * Статус задач обновлен");
        }

        ManagerAction::SendWork => {
            let mut manager = Manager::new();
            manager.workers.push("test-worker".to_string());
            manager.send_work();
            println!(" * Работа отправлена воркеру");
        }

        ManagerAction::Info => {
            let manager = Manager::new();
            println!(" * Информация о менеджере:");
            println!("   Воркеры в пуле: {:?}", manager.workers);
            println!("   Всего воркеров: {}", manager.workers.len());
        }
    }
}

async fn handle_node_action(action: NodeAction) {
    match action {
        NodeAction::Create {
            name,
            ip,
            cores,
            memory,
            disk,
            node_type,
        } => {
            let node = Node::new(name.clone(), ip, cores, memory, disk, node_type.clone());
            println!(" * Нода создана:");
            println!("   Имя: {}", node.name);
            println!("   IP: {}", node.ip);
            println!("   Ядра: {}", node.cores);
            println!("   Память: {} MB", node.memory);
            println!("   Диск: {} GB", node.disk);
            println!("   Тип: {}", node.role);
        }

        NodeAction::Info { name } => {
            let node = Node::new(
                name.clone(),
                "192.168.1.1".to_string(),
                4,
                1024,
                25,
                "worker".to_string(),
            );
            println!(" * Информация о ноде:");
            println!("   Имя: {}", node.name);
            println!("   IP: {}", node.ip);
            println!("   Ядра: {}", node.cores);
            println!("   Память: {} MB", node.memory);
            println!("   Диск: {} GB", node.disk);
            println!("   Тип: {}", node.role);
        }
    }
}

// Обновленный тестовый сценарий
async fn run_test_scenario() {
    println!(" * ЗАПУСК ТЕСТОВОГО СЦЕНАРИЯ");
    println!("================================\n");

    // Создаем задачу
    let task = Task::new(
        Uuid::new_v4(),
        "Task-1".to_string(),
        TaskState::Pending,
        "postgres:16".to_string(),
        1024 * 1024 * 256,
        1,
    );

    // Создаем событие задачи
    let task_event = TaskEvent::new(Uuid::new_v4(), TaskState::Pending, Utc::now(), task.clone());

    println!(" > task: {:?}", task);
    println!(" > task event: {:?}\n", task_event);

    // Создаем worker
    let mut worker = Worker::new("worker-1".to_string());
    println!(" > worker: {:?}", worker);
    worker.collect_stats().await;

    // Добавляем задачу в очередь и БД
    worker.add_task(task.clone());
    let task_arc = Arc::new(Mutex::new(task.clone()));
    worker.db.insert(task.id, task_arc.clone());

    println!(" > Running task...");
    worker.run_task().await;
    println!(" > Starting task...");
    worker.start_task(task_arc.clone()).await;
    println!(" > Stopping task...");
    worker.stop_task(task_arc).await;

    // Создаем manager
    let mut manager = Manager::new();
    manager.workers.push(worker.name.clone());
    println!("\n > manager: {:?}", manager);
    manager.select_worker();
    manager.update_tasks();
    manager.send_work();

    // Создаем node
    let node = Node::new(
        "Node-1".to_string(),
        "192.168.1.1".to_string(),
        4,
        1024,
        25,
        "worker".to_string(),
    );
    println!("\n > node: {:?}\n", node);

    // Тестируем создание контейнера
    println!(" * Создание тестового контейнера...");

    let config = ContainerConfig::new("postgres:16".to_string(), "test-container-1".to_string())
        .with_env(vec![
            "POSTGRES_USER=cube".to_string(),
            "POSTGRES_PASSWORD=secret".to_string(),
        ]);

    match DockerContainer::new(config).await {
        Ok(mut docker) => {
            let result = docker.run().await;

            if result.is_success() {
                println!(" * Контейнер запущен с ID: {}", result.container_id);

                println!(" * Ожидание 20 секунд...");
                tokio::time::sleep(Duration::from_secs(20)).await;

                println!(" * Остановка контейнера {}", result.container_id);
                match stop_container(&docker, &result.container_id).await {
                    Some(stop_result) => {
                        if stop_result.is_success() {
                            println!(" * Контейнер успешно остановлен и удален");
                        } else {
                            println!(" X Ошибка при остановке контейнера");
                        }
                    }
                    None => println!(" X Не удалось остановить контейнер"),
                }
            } else {
                println!(" X Ошибка при создании контейнера");
                if let Some(error) = result.error {
                    println!("   Ошибка: {}", error);
                }
            }
        }
        Err(e) => {
            eprintln!(" X Ошибка при создании Docker клиента: {}", e);
        }
    }

    println!("\n * ТЕСТОВЫЙ СЦЕНАРИЙ ЗАВЕРШЕН");
}
