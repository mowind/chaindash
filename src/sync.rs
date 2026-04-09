use std::sync::{
    Mutex,
    MutexGuard,
};

pub fn lock_or_panic<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("mutex poisoned")
}
