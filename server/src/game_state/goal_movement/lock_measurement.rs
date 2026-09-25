use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Copy)]
pub(super) struct LockSample {
    pub wait: Duration,
    pub hold: Duration,
}

#[derive(Default)]
pub(super) struct LockMeasurement {
    enabled: AtomicBool,
    samples: std::sync::Mutex<Vec<LockSample>>,
}

pub(super) struct LockTiming {
    owner: Arc<LockMeasurement>,
    acquired: Instant,
    wait: Duration,
}

impl LockMeasurement {
    pub(super) fn start_measurement(&self) {
        self.samples.lock().unwrap().clear();
        self.enabled.store(true, Ordering::Relaxed);
    }

    pub(super) fn finish_measurement(&self) -> Vec<LockSample> {
        self.enabled.store(false, Ordering::Relaxed);
        std::mem::take(&mut *self.samples.lock().unwrap())
    }

    pub(super) fn start(&self) -> Option<Instant> {
        self.enabled.load(Ordering::Relaxed).then(Instant::now)
    }

    pub(super) fn acquired(self: &Arc<Self>, started: Option<Instant>) -> Option<LockTiming> {
        started.map(|started| {
            let acquired = Instant::now();
            LockTiming {
                owner: self.clone(),
                acquired,
                wait: acquired.duration_since(started),
            }
        })
    }
}

impl Drop for LockTiming {
    fn drop(&mut self) {
        self.owner.samples.lock().unwrap().push(LockSample {
            wait: self.wait,
            hold: self.acquired.elapsed(),
        });
    }
}

pub(in crate::game_state) struct MovementMutex<T> {
    inner: Arc<Mutex<T>>,
    measurement: Arc<LockMeasurement>,
}

pub(in crate::game_state) struct MovementOwnedMutexGuard<T> {
    inner: OwnedMutexGuard<T>,
    _timing: Option<LockTiming>,
}

impl<T> MovementMutex<T> {
    pub(super) fn measured(value: T, measurement: Arc<LockMeasurement>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(value)),
            measurement,
        }
    }

    pub async fn lock_owned(self: Arc<Self>) -> MovementOwnedMutexGuard<T> {
        let started = self.measurement.start();
        let inner = self.inner.clone().lock_owned().await;
        MovementOwnedMutexGuard {
            inner,
            _timing: self.measurement.acquired(started),
        }
    }

    pub fn try_lock(&self) -> Result<tokio::sync::MutexGuard<'_, T>, tokio::sync::TryLockError> {
        self.inner.try_lock()
    }
}

impl<T> Deref for MovementOwnedMutexGuard<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for MovementOwnedMutexGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}
