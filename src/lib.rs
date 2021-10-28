extern crate futures_core;
extern crate pin_project;

use std::{pin::Pin, task::{Context, Poll}};

use futures_core::Stream;
use pin_project::{pin_project, pinned_drop};

// TODO: ?Sized

#[pin_project]
struct Tee<T> {
    buf: Option<T>,
    num_readers: usize,
    buf_read_by: usize,
    #[pin]
    input: Pin<Box<dyn Stream<Item = T> + Unpin>>, // Can Pin be eliminated here?
}

// impl<T> Unpin for Tee<T> {}

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
    pub fn create_output(&'a mut self) -> TeeOutput<'a, T> {
        if !self.buf_can_be_discarded() {
            self.buf_read_by += 1; // FIXME
        }
        TeeOutput::new(self)
    }
    pub fn buf_can_be_discarded(&self) -> bool {
        self.buf_read_by == self.num_readers
    }
    // fn fetch_buf(&mut self) -> Option<T> {
    //     assert!(self.buf_read_by < self.num_readers);
    //     self.buf_read_by += 1;
    //     self.buf
    // }
    fn take_buf(&mut self) -> Option<T> {
        assert!(self.buf_can_be_discarded());
        let result = self.buf;
        self.buf = None;
        self.buf_read_by = 0; // FIXME: or `= self.num_readers`?
        result
    }
}

#[pin_project(PinnedDrop)]
struct TeeOutput<'a, T> {
    #[pin]
    source: &'a mut Tee<T>, // TODO: Can we get rid of this Pin?
    has_delivered_buf: bool,
}

// impl<'a, T> Unpin for TeeOutput<'a, T> { }


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
        self.source.input.size_hint() // TODO: +1?
    }
}
