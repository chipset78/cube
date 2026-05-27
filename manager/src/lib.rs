use std::collections::{HashMap, VecDeque};
use task::{Task, TaskEvent};
use uuid::Uuid;

/// Manager
#[derive(Debug, Default)]
pub struct Manager {
    pub pending: VecDeque<Task>,
    pub task_db: HashMap<String, Vec<Task>>,
    pub event_db: HashMap<String, Vec<TaskEvent>>,
    pub workers: Vec<String>,
    pub worker_task_map: HashMap<String, Vec<Uuid>>,
    pub task_worker_map: HashMap<Uuid, String>,
}

impl Manager {
    /// Создание нового Manager
    pub fn new() -> Self {
        Manager {
            pending: VecDeque::new(),
            task_db: HashMap::new(),
            event_db: HashMap::new(),
            workers: Vec::new(),
            worker_task_map: HashMap::new(),
            task_worker_map: HashMap::new(),
        }
    }

    /// SelectWorker - выбирает подходящего worker'а
    pub fn select_worker(&self) {
        println!("I will select an appropriate worker");
    }

    /// UpdateTasks - обновляет задачи
    pub fn update_tasks(&self) {
        println!("I will update tasks");
    }

    /// SendWork - отправляет работу worker'ам
    pub fn send_work(&self) {
        println!("I will send work to workers");
    }
}
