use async_task::{JoinHandle, Task};
use std::future::Future;
use std::sync::Mutex;

pub type SpawnFunc = Box<dyn FnOnce() + Send>;
pub type ScheduleFunc = Box<dyn Fn(Task<()>) + Send + Sync + 'static>;

fn no_schedule_configured(_: Task<()>) {
    panic!("no scheduler has been configured");
}

lazy_static::lazy_static! {
    static ref ON_MAIN_THREAD: Mutex<ScheduleFunc> = Mutex::new(Box::new(no_schedule_configured));
    static ref ON_MAIN_THREAD_LOW_PRI: Mutex<ScheduleFunc> = Mutex::new(Box::new(no_schedule_configured));
}

pub fn set_schedulers(main: ScheduleFunc, low_pri: ScheduleFunc) {
    *ON_MAIN_THREAD.lock().unwrap() = Box::new(main);
    *ON_MAIN_THREAD_LOW_PRI.lock().unwrap() = Box::new(low_pri);
}

pub fn spawn<F, R>(future: F) -> JoinHandle<R, ()>
where
    F: Future<Output = R> + 'static,
    R: 'static,
{
    let (task, handle) =
        async_task::spawn_local(future, |task| ON_MAIN_THREAD.lock().unwrap()(task), ());
    task.schedule();
    handle
}

pub fn spawn_into_main_thread<F, R>(future: F) -> JoinHandle<R, ()>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let (task, handle) = async_task::spawn(future, |task| ON_MAIN_THREAD.lock().unwrap()(task), ());
    task.schedule();
    handle
}

pub fn spawn_into_main_thread_with_low_priority<F, R>(future: F) -> JoinHandle<R, ()>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let (task, handle) =
        async_task::spawn(future, |task| ON_MAIN_THREAD_LOW_PRI.lock().unwrap()(task), ());
    task.schedule();
    handle
}
