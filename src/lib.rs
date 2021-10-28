extern crate futures_core;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;

// TODO: ?Sized

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

impl<'a, T: Copy> Tee<'a, T> {
    pub fn create_output(&'a mut self, n: usize) -> TeeOutput<'a, T> {
        if !self.buf_can_be_discarded() {
            self.buf_read_by += 1; // FIXME
        }
        TeeOutput::new(self)
    }
    pub fn buf_can_be_discarded(&self) -> bool {
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
    source: &'a mut Pin<Tee<'a, T>>,
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
    fn new(source: &'a mut Tee<'a, T>) -> TeeOutput<'a, T> {
        TeeOutput {
            source,
            has_delivered_buf: false,
        }
    }
}

impl<'a, T: Copy> Stream for TeeOutput<'a, T> {
    type Item = T;

    fn poll_next(
        self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<Option<Self::Item>> {
        let source = self.source;
        if self.has_delivered_buf {
            if source.buf_can_be_discarded() {
                match self.poll_next(cx) {
                    Poll::Pending => {
                        source.buf = None; // needed?
                        Poll::Pending
                    },
                    Poll::Ready(val) => {
                        source.buf = val;
                        assert!(source.buf_can_be_discarded());
                        source.buf_read_by = 1;
                        self.has_delivered_buf = true;
                        Poll::Ready(val)
                    },
                }
            } else {
                Poll::Pending
            }
        } else {
            Poll::Ready(source.take_buf())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source.input.size_hint() // TODO: +1?
    }
}
