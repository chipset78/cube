use std::collections::{HashMap, VecDeque};
use task::Task;
use uuid::Uuid;

/// Worker
#[derive(Debug)] //, Clone, Serialize, Deserialize, Default)]
pub struct Worker {
    pub name: String,
    pub queue: VecDeque<Task>,
    pub db: HashMap<Uuid, Task>,
    pub task_count: i32,
}

impl Worker {
    /// Создание нового worker
    pub fn new(name: String) -> Self {
        Worker {
            name,
            queue: VecDeque::new(),
            db: HashMap::new(),
            task_count: 0,
        }
    }

    /// collect_stats - просто выводит сообщение
    pub fn collect_stats(&self) {
        println!("I will collect stats");
    }

    /// run_task - запускает или останавливает задачу
    pub fn run_task(&self) {
        println!("I will start or stop a task");
    }

    /// start_task - запускает задачу
    pub fn start_task(&self) {
        println!("I will start a task");
    }

    /// stop_task - останавливает задачу
    pub fn stop_task(&self) {
        println!("I will stop a task");
    }
}
