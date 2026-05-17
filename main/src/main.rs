use chrono::Utc;
use uuid::Uuid;

use manager::Manager;
use node::Node;
use task::{Task, TaskEvent, TaskState, create_container, stop_container};
use worker::Worker;

use std::time::Duration;

#[tokio::main]
async fn main() {
    // Создаем задачу
    let task = Task::new(
        Uuid::new_v4(),
        "Task-1".to_string(),
        TaskState::Pending,
        "Image-1".to_string(),
        1024, // memory
        1,    // disk
    );

    // Создаем событие задачи
    let task_event = TaskEvent::new(Uuid::new_v4(), TaskState::Pending, Utc::now(), task.clone());

    println!("task: {:?}", task);
    println!("task event: {:?}", task_event);

    // Создаем worker
    let worker = Worker::new("worker-1".to_string());

    println!("worker: {:?}", worker);
    worker.collect_stats();
    worker.run_task();
    worker.start_task();
    worker.stop_task();

    // Создаем manager
    let mut manager = Manager::new();
    manager.workers.push(worker.name.clone());

    println!("manager: {:?}", manager);
    manager.select_worker();
    manager.update_tasks();
    manager.send_work();

    // Создаем node
    let node = Node::new(
        "Node-1".to_string(),
        "192.168.1.1".to_string(),
        4,    // cores
        1024, // memory
        25,   // disk
        "worker".to_string(),
    );

    println!("node: {:?}", node);

    println!("create a test container");
    let (docker_task, create_result) = match create_container().await {
        Ok(result) => result,
        Err(e) => {
            println!("Error creating container: {}", e);
            std::process::exit(1);
        }
    };

    if create_result.error.is_some() {
        println!("{:?}", create_result.error);
        std::process::exit(1);
    }

    println!("Container started with ID: {}", create_result.container_id);

    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("stopping container {}", create_result.container_id);
    let _ = stop_container(&docker_task, &create_result.container_id).await;
}
