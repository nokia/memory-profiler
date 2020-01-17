use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use futures;

struct Inner<T> {
    buffer: VecDeque<T>,
    task: Option<futures::task::Task>,
    sender_closed: bool,
    receiver_closed: bool,
}

pub struct Sender<T>(Arc<(Condvar, Mutex<Inner<T>>)>);

impl<T> Sender<T> {
    pub fn send(&mut self, value: T) -> Result<(), ()> {
        let mut inner = (self.0).1.lock().unwrap();
        if inner.receiver_closed {
            inner.buffer.clear();
            return Err(());
        }

        while inner.buffer.len() >= 16 {
            inner = (self.0).0.wait(inner).unwrap();
            if inner.receiver_closed {
                inner.buffer.clear();
                return Err(());
            }
        }

        inner.buffer.push_back(value);

        if let Some(ref mut task) = inner.task {
            task.notify();
        }

        Ok(())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut inner = (self.0).1.lock().unwrap();
        inner.sender_closed = true;
        if let Some(ref mut task) = inner.task {
            task.notify();
        }
    }
}

pub struct Receiver<T>(Arc<(Condvar, Mutex<Inner<T>>)>);

impl<T> futures::Stream for Receiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let mut inner = (self.0).1.lock().unwrap();
        match inner.buffer.pop_front() {
            Some(value) => {
                (self.0).0.notify_all();
                Ok(futures::Async::Ready(Some(value)))
            }
            None => {
                if inner.sender_closed {
                    return Ok(futures::Async::Ready(None));
                }

                inner.task = Some(futures::task::current());
                Ok(futures::Async::NotReady)
            }
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut inner = (self.0).1.lock().unwrap();
        inner.receiver_closed = true;
        (self.0).0.notify_all();
    }
}

pub fn streaming_channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Inner {
        buffer: VecDeque::new(),
        task: None,
        sender_closed: false,
        receiver_closed: false,
    };

    let condvar = Condvar::new();
    let inner = Arc::new((condvar, Mutex::new(inner)));
    let tx = Sender(inner.clone());
    let rx = Receiver(inner.clone());

    (tx, rx)
}
