/// Node - представляет узел (worker ноду) в кластере
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub ip: String,
    pub cores: i32,
    pub memory: i32,
    pub memory_allocated: i32,
    pub disk: i32,
    pub disk_allocated: i32,
    pub role: String,
    pub task_count: i32,
}

impl Node {
    /// Создание новой ноды
    pub fn new(name: String, ip: String, cores: i32, memory: i32, disk: i32, role: String) -> Self {
        Node {
            name,
            ip,
            cores,
            memory,
            memory_allocated: 0,
            disk,
            disk_allocated: 0,
            role,
            task_count: 0,
        }
    }
}
