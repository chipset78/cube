/// Scheduler
pub trait Scheduler {
    fn select_candidate_nodes(&self);
    fn score(&self);
    fn pick(&self);
}

/// Пустая реализация для примера (можно удалить потом)
pub struct SimpleScheduler;

impl Scheduler for SimpleScheduler {
    fn select_candidate_nodes(&self) {
        println!("Selecting candidate nodes");
    }

    fn score(&self) {
        println!("Scoring nodes");
    }

    fn pick(&self) {
        println!("Picking best node");
    }
}
