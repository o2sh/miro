use failure::{Error, Fallible};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll};

type NextFunc<T> = Box<dyn FnOnce(Fallible<T>) + Send>;
pub type SpawnFunc = Box<dyn FnOnce() + Send>;

#[derive(Debug, Fail)]
#[fail(display = "Promise was dropped before completion")]
pub struct BrokenPromise {}

pub trait BasicExecutor {
    fn execute(&self, f: SpawnFunc);
}

pub trait Executor: BasicExecutor + Send {
    fn clone_executor(&self) -> Box<dyn Executor>;
}

impl BasicExecutor for Box<dyn Executor> {
    fn execute(&self, f: SpawnFunc) {
        BasicExecutor::execute(&**self, f)
    }
}

impl Executor for Box<dyn Executor> {
    fn clone_executor(&self) -> Box<dyn Executor> {
        Executor::clone_executor(&**self)
    }
}

enum PromiseState<T> {
    Waiting(Arc<Core<T>>),
    Fulfilled,
}

enum FutureState<T> {
    Waiting(Arc<Core<T>>),
    Ready(Result<T, Error>),
    Resolved,
}

struct CoreData<T> {
    result: Option<Result<T, Error>>,
    propagate: Option<NextFunc<T>>,
}

struct Core<T> {
    data: Mutex<CoreData<T>>,
    cond: Condvar,
}

pub struct Promise<T> {
    state: PromiseState<T>,
    future: Option<Future<T>>,
}

pub struct Future<T> {
    state: FutureState<T>,
}

impl<T> Default for Promise<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        if let PromiseState::Waiting(core) = &mut self.state {
            let err = Err(BrokenPromise {}.into());
            let mut locked = core.data.lock().unwrap();
            if let Some(func) = locked.propagate.take() {
                func(err);
            } else {
                locked.result = Some(err);
            }
            core.cond.notify_one();
        }
    }
}

impl<T> Promise<T> {
    pub fn new() -> Self {
        let core = Arc::new(Core {
            data: Mutex::new(CoreData { result: None, propagate: None }),
            cond: Condvar::new(),
        });

        Self {
            state: PromiseState::Waiting(Arc::clone(&core)),
            future: Some(Future { state: FutureState::Waiting(core) }),
        }
    }

    pub fn get_future(&mut self) -> Option<Future<T>> {
        self.future.take()
    }

    pub fn result(&mut self, result: Result<T, Error>) {
        match std::mem::replace(&mut self.state, PromiseState::Fulfilled) {
            PromiseState::Waiting(core) => {
                let mut locked = core.data.lock().unwrap();
                match locked.propagate.take() {
                    Some(func) => func(result),
                    None => locked.result = Some(result),
                }
                core.cond.notify_one();
            }
            PromiseState::Fulfilled => panic!("Promise already fulfilled"),
        }
    }
}

impl<T: Send + 'static> std::convert::From<Result<T, Error>> for Future<T> {
    fn from(result: Result<T, Error>) -> Future<T> {
        Future::result(result)
    }
}

impl<T: Send + 'static> Future<T> {
    /// Create a leaf future which is immediately ready with
    /// the provided error
    pub fn err(err: Error) -> Self {
        Self::result(Err(err))
    }

    /// Create a leaf future which is immediately ready with
    /// the provided result
    pub fn result(result: Result<T, Error>) -> Self {
        Self { state: FutureState::Ready(result) }
    }

    /// Create a future from a function that will be spawned via
    /// the provided executor
    pub fn with_executor<F, IF, EXEC>(executor: EXEC, f: F) -> Future<T>
    where
        F: FnOnce() -> IF + Send + 'static,
        IF: Into<Future<T>> + 'static,
        EXEC: BasicExecutor,
    {
        let mut promise = Promise::new();
        let future = promise.get_future().unwrap();

        let func = Box::new(f);
        let promise_chain = Box::new(move |result| promise.result(result));
        executor.execute(Box::new(move || {
            let future = func().into();
            future.chain(promise_chain);
        }));
        future
    }

    fn chain(self, f: NextFunc<T>) {
        match self.state {
            FutureState::Ready(result) => {
                f(result);
            }
            FutureState::Waiting(core) => {
                let mut locked = core.data.lock().unwrap();
                if let Some(result) = locked.result.take() {
                    f(result);
                } else {
                    locked.propagate = Some(f);
                }
            }
            FutureState::Resolved => panic!("cannot chain a Resolved future"),
        }
    }

    /// Blocks until the associated promise is fulfilled
    pub fn wait(self) -> Result<T, Error> {
        match self.state {
            FutureState::Waiting(core) => {
                let mut locked = core.data.lock().unwrap();
                loop {
                    if let Some(result) = locked.result.take() {
                        return result;
                    }
                    locked = core.cond.wait(locked).unwrap();
                }
            }
            FutureState::Ready(result) => result,
            FutureState::Resolved => failure::bail!("Future is already Resolved"),
        }
    }

    pub fn is_ready(&self) -> bool {
        match &self.state {
            FutureState::Waiting(core) => {
                let locked = core.data.lock().unwrap();
                locked.result.is_some()
            }
            FutureState::Ready(_) | FutureState::Resolved => true,
        }
    }

    /// When this future resolves, then map the result via the
    /// supplied lambda, which returns something that is convertible
    /// to a Future.
    pub fn then<U, F, IF>(self, f: F) -> Future<U>
    where
        F: FnOnce(Result<T, Error>) -> IF + Send + 'static,
        IF: Into<Future<U>> + 'static,
        U: Send + 'static,
    {
        let mut promise = Promise::new();
        let future = promise.get_future().unwrap();
        let func = Box::new(f);

        let promise_chain = Box::new(move |result| promise.result(result));

        self.chain(Box::new(move |result| {
            let future = func(result).into();
            future.chain(promise_chain);
        }));
        future
    }
}

impl<T: Send + 'static> std::future::Future for Future<T> {
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context) -> Poll<Self::Output> {
        // This should be safe because we're not moving the Future,
        // but instead replacing a field, and since no one is able to
        // reference the state field, we should be ok with moving that.
        let myself = unsafe { Pin::get_unchecked_mut(self) };

        let state = std::mem::replace(&mut myself.state, FutureState::Resolved);
        match state {
            FutureState::Waiting(core) => {
                let mut locked = core.data.lock().unwrap();
                if let Some(result) = locked.result.take() {
                    return Poll::Ready(result);
                }
                drop(locked);
                myself.state = FutureState::Waiting(core);
                Poll::Pending
            }
            FutureState::Ready(result) => Poll::Ready(result),
            FutureState::Resolved => panic!("polling a Resolved Future"),
        }
    }
}
