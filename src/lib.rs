extern crate futures_core;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;

struct Tee<'a, T> {
    buf: Option<T>,
    num_readers: usize,
    buf_read_by: usize,
    input: &'a dyn Stream<Item = T>,
}

impl<'a, T> Tee<'a, T> {
    pub fn new(input: &'a dyn Stream<Item = T>, num: usize) -> Self {
        Self {
            buf: None,
            num_readers: 0,
            buf_read_by: 0,
            input,
        }
    }
}

impl<'a, T> Tee<'a, T> {
    pub fn create_output(&'a self, n: usize) -> TeeOutput<'a, T> {
        if !self.buf_can_be_discarded() {
            self.buf_read_by += 1; // FIXME
        }
        TeeOutput::new(&self)
    }
    fn buf_can_be_discarded(&self) -> bool {
        self.buf_read_by == self.num_readers
    }
    fn fetch_buf(&mut self) -> Option<T> {
        assert!(self.buf_read_by < self.num_readers);
        self.buf_read_by += 1;
        self.buf
    }
    fn take_buf(&mut self) -> Option<T> {
        assert!(self.buf_can_be_discarded());
        let result = self.buf;
        self.buf = None;
        self.buf_read_by = 0; // FIXME: or `= self.num_readers`?
        result
    }
}

struct TeeOutput<'a, T> {
    source: &'a Tee<'a, T>,
    has_delivered_buf: bool,
}

impl<'a, T> Drop for TeeOutput<'a, T> {
    fn drop(&mut self) {
        self.source.num_readers -= 1;
        if self.has_delivered_buf {
            self.source.buf_read_by -= 1;
        }
        assert!(self.source.buf_read_by <= self.source.num_readers);
    }
}

impl<'a, T> TeeOutput<'a, T> {
    pub fn new(source: &'a Tee<'a, T>) -> TeeOutput<'a, T> {
        TeeOutput {
            source,
            has_delivered_buf: false,
        }
    }
}

impl<'a, T> Stream<Item = T> for TeeOutput<'a, T> {
    type Item = T;

    fn poll_next(
        self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<Option<Self::Item>> {
        if self.has_delivered_buf {
            if self.source.buf_can_be_discarded() {
                match self.poll_next(cx) {
                    Poll::Pending => {
                        self.source.buf = None; // needed?
                        Poll::Pending
                    },
                    Poll::Ready(val) => {
                        let val = Some(val);
                        self.source.buf = val;
                        assert!(self.buf_can_be_discarded());
                        self.source.buf_read_by = 1;
                        self.has_delivered_buf = true;
                        Poll::Ready(val)
                    },
                }
            } else {
                Poll::Pending
            }
        } else {
            Poll::Ready(self.source.take_buf())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source.size_hint() // TODO: +1?
    }
}
