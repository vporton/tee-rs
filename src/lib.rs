extern crate futures_core;
extern crate pin_project;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;
use pin_project::pin_project;

// TODO: ?Sized

// #[pin_project]
struct Tee<'a, T> {
    buf: Option<T>,
    num_readers: usize,
    buf_read_by: usize,
    // #[pin]
    input: Pin<Box<&'a mut dyn Stream<Item = T>>>, // Can Pin be eliminated here?
}

impl<'a, T> Tee<'a, T> {
    pub fn new(input: &'a mut dyn Stream<Item = T>) -> Self {
        Self {
            buf: None,
            num_readers: 0,
            buf_read_by: 0,
            input: Box::pin(input),
        }
    }
}

impl<'a, T: Copy> Tee<'a, T> {
    pub fn create_output(&'a mut self, n: usize) -> TeeOutput<T> {
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
    source: &'a mut Tee<'a, T>, // TODO: Can we get rid of this Pin?
    has_delivered_buf: bool,
}

impl<'a, T> Unpin for TeeOutput<'a, T> { }

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
    fn pin_get_source(self: Pin<&'a mut Self>) -> Pin<&'a mut Tee<'a, T>> {
        unsafe { self.map_unchecked_mut(|s| s.source) }
    }
    fn new<'b>(source: &'b mut Tee<'b, T>) -> TeeOutput<'b, T> {
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
        let mut input = self.pin_get_source().input;
        // let mut input = Pin::new(self.source).project().input;
        // let mut input = Box::pin(self.source.input);
        if self.has_delivered_buf {
            if self.source.buf_can_be_discarded() {
                // match input.poll_next(cx) {
                match <Pin<Box<&mut dyn Stream<Item = T>>>>::poll_next(input, cx) {
                    Poll::Pending => {
                        self.source.buf = None; // needed?
                        Poll::Pending
                    },
                    Poll::Ready(val) => {
                        self.source.buf = val;
                        assert!(self.source.buf_can_be_discarded());
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
        self.source.input.size_hint() // TODO: +1?
    }
}
