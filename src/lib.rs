extern crate futures_core;
extern crate pin_project;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;
use pin_project::{pin_project, pinned_drop};

#[pin_project]
pub struct Tee<T> {
    buf: Option<T>,
    num_readers: usize,
    buf_read_by: usize,
    #[pin]
    input: Box<dyn Stream<Item = T> + Unpin>, // Can Pin be eliminated here?
}

impl<T> Tee<T> {
    pub fn new(input: Box<dyn Stream<Item = T> + Unpin>) -> Self {
        Self {
            buf: None,
            num_readers: 0,
            buf_read_by: 0,
            input,
        }
    }
    pub fn num_readers(&self) -> usize {
        self.num_readers
    }
    pub fn input_stream(&self) -> &dyn Stream<Item = T> {
        &*self.input
    }
}

impl<'a, T: Copy> Tee<T> {
    pub fn create_output(&'a mut self) -> TeeOutput<'a, T> {
        if !self.buf_can_be_discarded() {
            self.buf_read_by += 1;
        }
        TeeOutput::new(self)
    }
    fn buf_can_be_discarded(&self) -> bool {
        self.buf_read_by == self.num_readers
    }
    fn take_buf(&mut self) -> Option<T> {
        assert!(self.buf_read_by + 1 == self.num_readers);
        self.buf_read_by = self.num_readers;
        assert!(self.buf_can_be_discarded());
        let result = self.buf;
        self.buf = None;
        result
    }
}

#[pin_project(PinnedDrop)]
pub struct TeeOutput<'a, T> {
    #[pin]
    source: &'a mut Tee<T>,
    has_delivered_buf: bool,
}

#[pinned_drop]
impl<'a, T> PinnedDrop for TeeOutput<'a, T> {
    fn drop(self: Pin<&mut Self>) {
        let mut this = self.project();
        this.source.num_readers -= 1;
        if *this.has_delivered_buf {
            this.source.buf_read_by -= 1;
        }
        assert!(this.source.buf_read_by <= this.source.num_readers);
    }
}

impl<'a, T> TeeOutput<'a, T> {
    fn new<'b>(source: &mut Tee<T>) -> TeeOutput<T> { // private!
        TeeOutput {
            source,
            has_delivered_buf: false,
        }
    }
    pub fn source(&mut self) -> &mut Tee<T> {
        self.source
    }
}

impl<'a, T: Copy> Stream for TeeOutput<'a, T> {
    type Item = T;

    fn poll_next(
        self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let source0 = this.source;
        let mut source = unsafe { source0.map_unchecked_mut(|s| *s) }; // Correct?
        if *this.has_delivered_buf {
            // if source1.buf_can_be_discarded() { // does not compile
            if source.buf_read_by as usize == source.num_readers as usize { // I added `usize` to be sure  compare not pointers.
                match Pin::new(&mut source.input).poll_next(cx) {
                    Poll::Pending => {
                        source.buf = None; // needed?
                        Poll::Pending
                    },
                    Poll::Ready(val) => {
                        source.buf = val;
                        // assert!(source1.buf_can_be_discarded());
                        source.buf_read_by = 1;
                        *this.has_delivered_buf = true;
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
        let result = self.source.input.size_hint();
        if self.has_delivered_buf || self.source.buf.is_none() {
            result
        } else {
            (result.0 + 1, result.1.map(|s| s + 1))
        }
    }
}
