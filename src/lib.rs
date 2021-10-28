extern crate futures_core;
extern crate pin_project;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;
use pin_project::pin_project;

// TODO: ?Sized

#[pin_project]
struct Tee<T> {
    buf: Option<T>,
    num_readers: usize,
    buf_read_by: usize,
    #[pin]
    input: Pin<Box<dyn Stream<Item = T> + Unpin>>, // Can Pin be eliminated here?
}

impl<T> Tee<T> {
    pub fn new(input: Box<dyn Stream<Item = T> + Unpin>) -> Self {
        Self {
            buf: None,
            num_readers: 0,
            buf_read_by: 0,
            input: Pin::new(input),
        }
    }
}

impl<'a, T: Copy> Tee<T> {
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
    source: &'a mut Tee<T>, // TODO: Can we get rid of this Pin?
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
    fn pin_get_source(self: Pin<&mut Self>) -> Pin<&mut Tee<T>> {
        unsafe { self.map_unchecked_mut(|s| s.source) }
    }
    fn new<'b>(source: &mut Tee<T>) -> TeeOutput<T> {
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
        let mut source = self.pin_get_source();
        let mut input = &mut source.project().input;
        // let mut input = Pin::new(self.source).project().input;
        // let mut input = Box::pin(self.source.input);
        if self.has_delivered_buf {
            if self.source.buf_can_be_discarded() {
                match Pin::new(&mut input).poll_next(cx) {
                // match <Pin<Box<&mut dyn Stream<Item = T> + Unpin>>>::poll_next(input, cx) {
                    Poll::Pending => {
                        source.buf = None; // needed?
                        Poll::Pending
                    },
                    Poll::Ready(val) => {
                        source.buf = val;
                        assert!(self.source.buf_can_be_discarded());
                        source.buf_read_by = 1;
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
