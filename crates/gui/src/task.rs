//! Work started in the background for the interface, and what it asks of the window (scroll a
//! list, move the focus, read the clipboard, go full screen…), as Slatty's own type: the state and
//! its messages do not depend on the interface framework, and `into_iced` hands it to Iced.
//!
//! It works as Iced's `Task` does, which the tests rely on: a stream of outputs and of requests to
//! the window, run with the others of a batch at once; `then` runs a task for each output, one
//! after the other; the answer to a request (the clipboard's text, the window's scale, where a part
//! of the page is) comes back as an output, so that `map` and `then` never look into a request.

use std::future::Future;

use futures::FutureExt;
use futures::channel::oneshot;
use futures::stream::{self, BoxStream, Stream, StreamExt};

/// What a task asks of the window.
pub enum Effect {
    /// Scrolls the scrollable `id` to this offset (an axis left out stays as it is).
    ScrollTo {
        id: &'static str,
        x: Option<f32>,
        y: Option<f32>,
    },
    /// Scrolls the scrollable `id` to its end.
    SnapToEnd(&'static str),
    /// Moves the keyboard's focus to the next field, or the one before.
    FocusNext,
    FocusPrevious,
    /// Shows the window full screen, or back in its frame.
    Fullscreen(bool),
    /// Closes the window and quits.
    Exit,
    ReadClipboard(oneshot::Sender<Option<String>>),
    /// The window's scale.
    ScaleFactor(oneshot::Sender<f32>),
    /// Where the tops of the parts of the page with these ids are, in the order asked, `None` for
    /// one not shown.
    Tops(Vec<&'static str>, oneshot::Sender<Vec<Option<f32>>>),
}

/// An output of a task, or a request to the window.
pub enum Action<T> {
    Output(T),
    Effect(Effect),
}

/// Work for the interface: its outputs become messages.
#[must_use = "a task does nothing unless it is returned to the runtime"]
pub struct Task<T> {
    stream: Option<BoxStream<'static, Action<T>>>,
    units: usize,
}

impl<T> Task<T> {
    pub fn none() -> Self {
        Task {
            stream: None,
            units: 0,
        }
    }

    /// Produces `value` at once.
    pub fn done(value: T) -> Self
    where
        T: Send + 'static,
    {
        Self::future(std::future::ready(value))
    }

    /// Runs `future` to its end and maps its output with `f`.
    pub fn perform<A>(
        future: impl Future<Output = A> + Send + 'static,
        f: impl FnOnce(A) -> T + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
    {
        Self::future(future.map(f))
    }

    /// Runs `future` to its end.
    pub fn future(future: impl Future<Output = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Self::stream(stream::once(future))
    }

    /// Runs `stream` to its end and maps each item with `f`.
    pub fn run<A>(
        stream: impl Stream<Item = A> + Send + 'static,
        f: impl Fn(A) -> T + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
    {
        Self::stream(stream.map(f))
    }

    fn stream(stream: impl Stream<Item = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Task {
            stream: Some(stream.map(Action::Output).boxed()),
            units: 1,
        }
    }

    /// Asks `effect` of the window.
    pub fn effect(effect: Effect) -> Self
    where
        T: Send + 'static,
    {
        Task {
            stream: Some(stream::once(async move { Action::Effect(effect) }).boxed()),
            units: 1,
        }
    }

    /// Asks the window for something it answers: `effect` is made with the channel the answer
    /// comes back on, which becomes the task's output.
    fn ask<A: Send + 'static>(effect: impl FnOnce(oneshot::Sender<A>) -> Effect) -> Task<A> {
        let (sender, receiver) = oneshot::channel();
        let answer = receiver
            .into_stream()
            .filter_map(|a| async { a.ok().map(Action::Output) });
        Task {
            stream: Some(
                stream::once(std::future::ready(Action::Effect(effect(sender))))
                    .chain(answer)
                    .boxed(),
            ),
            units: 1,
        }
    }

    /// All of `tasks`, at once.
    pub fn batch(tasks: impl IntoIterator<Item = Self>) -> Self
    where
        T: Send + 'static,
    {
        let mut all = stream::SelectAll::new();
        let mut units = 0;
        for task in tasks {
            if let Some(stream) = task.stream {
                all.push(stream);
            }
            units += task.units;
        }
        Task {
            stream: (!all.is_empty()).then(|| all.boxed()),
            units,
        }
    }

    pub fn map<O: Send + 'static>(self, mut f: impl FnMut(T) -> O + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
    {
        self.then(move |output| Task::done(f(output)))
    }

    /// Runs the task `f` makes of each output, one after the other.
    pub fn then<O: Send + 'static>(
        self,
        mut f: impl FnMut(T) -> Task<O> + Send + 'static,
    ) -> Task<O>
    where
        T: Send + 'static,
    {
        Task {
            stream: self.stream.map(|stream| {
                stream
                    .flat_map(move |action| match action {
                        Action::Output(output) => {
                            f(output).stream.unwrap_or_else(|| stream::empty().boxed())
                        }
                        Action::Effect(effect) => {
                            stream::once(async move { Action::Effect(effect) }).boxed()
                        }
                    })
                    .boxed()
            }),
            units: self.units,
        }
    }

    /// Runs `task` once this one is over.
    pub fn chain(self, task: Self) -> Self
    where
        T: Send + 'static,
    {
        match self.stream {
            None => task,
            Some(first) => Task {
                stream: Some(match task.stream {
                    None => first,
                    Some(second) => first.chain(second).boxed(),
                }),
                units: self.units + task.units,
            },
        }
    }

    /// The same work, its outputs dropped.
    pub fn discard<O: Send + 'static>(self) -> Task<O>
    where
        T: Send + 'static,
    {
        self.then(|_| Task::none())
    }

    /// How many pieces of work the task holds, as Iced counts them.
    pub fn units(&self) -> usize {
        self.units
    }

    /// The task as Iced runs it.
    pub fn into_iced(self) -> iced::Task<T>
    where
        T: Send + 'static,
    {
        match self.stream {
            None => iced::Task::none(),
            Some(stream) => iced::Task::run(stream, |action| action).then(|action| match action {
                Action::Output(output) => iced::Task::done(output),
                Action::Effect(effect) => iced_effect(effect),
            }),
        }
    }
}

impl<T> Task<Option<T>> {
    /// Runs the task `f` makes of each output that is `Some`.
    pub fn and_then<A: Send + 'static>(self, f: impl Fn(T) -> Task<A> + Send + 'static) -> Task<A>
    where
        T: Send + 'static,
    {
        self.then(move |option| option.map_or_else(Task::none, &f))
    }
}

/// Reads the clipboard's text.
pub fn read_clipboard() -> Task<Option<String>> {
    Task::<()>::ask(Effect::ReadClipboard)
}

/// The window's scale.
pub fn scale_factor() -> Task<f32> {
    Task::<()>::ask(Effect::ScaleFactor)
}

/// Where the tops of the parts of the page with these ids are, `None` for one not shown.
pub fn tops(ids: Vec<&'static str>) -> Task<Vec<Option<f32>>> {
    Task::<()>::ask(|answer| Effect::Tops(ids, answer))
}

/// What Iced does for `effect`; an answer goes back on its channel.
fn iced_effect<T: Send + 'static>(effect: Effect) -> iced::Task<T> {
    use iced::widget::operation;
    use iced::widget::selector::{Target, find, id};
    // Iced's `then` takes a closure it may call more than once: each answer is sent once.
    fn answer<A: Send + 'static, T: Send + 'static>(
        sender: oneshot::Sender<A>,
    ) -> impl FnMut(A) -> iced::Task<T> + Send + 'static {
        let mut sender = Some(sender);
        move |a| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(a);
            }
            iced::Task::none()
        }
    }
    match effect {
        Effect::ScrollTo { id, x, y } => {
            operation::scroll_to(id, operation::AbsoluteOffset { x, y })
        }
        Effect::SnapToEnd(id) => operation::snap_to_end(id),
        Effect::FocusNext => operation::focus_next(),
        Effect::FocusPrevious => operation::focus_previous(),
        Effect::Fullscreen(on) => {
            let mode = if on {
                iced::window::Mode::Fullscreen
            } else {
                iced::window::Mode::Windowed
            };
            iced::window::oldest().and_then(move |window| iced::window::set_mode(window, mode))
        }
        Effect::Exit => iced::exit(),
        Effect::ReadClipboard(sender) => iced::clipboard::read().then(answer(sender)),
        Effect::ScaleFactor(sender) => iced::window::latest()
            .and_then(iced::window::scale_factor)
            .then(answer(sender)),
        Effect::Tops(ids, sender) => {
            let mut found = iced::Task::done(Vec::new());
            for one in ids {
                found = found.then(move |tops: Vec<Option<f32>>| {
                    find(id(one)).map(move |target: Option<Target>| {
                        let mut tops = tops.clone();
                        tops.push(target.map(|t| t.bounds().y));
                        tops
                    })
                });
            }
            found.then(answer(sender))
        }
    }
}

/// A stream of what `f` sends on the channel it is given, as long as it runs.
pub fn channel<T: Send + 'static, F: Future<Output = ()> + Send + 'static>(
    size: usize,
    f: impl FnOnce(futures::channel::mpsc::Sender<T>) -> F,
) -> impl Stream<Item = T> + Send + 'static {
    let (sender, receiver) = futures::channel::mpsc::channel(size);
    let runner = stream::once(f(sender)).filter_map(|()| async { None });
    stream::select(receiver, runner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outputs<T: Send + 'static>(task: Task<T>) -> Vec<T> {
        futures::executor::block_on(async {
            match task.stream {
                None => Vec::new(),
                Some(stream) => {
                    stream
                        .filter_map(|a| async {
                            match a {
                                Action::Output(o) => Some(o),
                                Action::Effect(_) => None,
                            }
                        })
                        .collect()
                        .await
                }
            }
        })
    }

    /// Units are counted as Iced counts them, which the interface's tests check.
    #[test]
    fn units_are_counted_as_iceds() {
        assert_eq!(Task::<u8>::none().units(), 0);
        assert_eq!(Task::done(1).units(), 1);
        assert_eq!(
            Task::batch([Task::done(1), Task::none(), Task::done(2)]).units(),
            2
        );
        assert_eq!(
            Task::done(1)
                .then(|_| Task::batch([Task::done(2), Task::done(3)]))
                .units(),
            1
        );
        assert_eq!(Task::done(1).chain(Task::done(2)).units(), 2);
        assert_eq!(Task::<u8>::effect(Effect::Exit).units(), 1);
    }

    #[test]
    fn outputs_go_through_map_then_and_chain() {
        assert_eq!(outputs(Task::done(2).map(|n| n * 10)), [20]);
        assert_eq!(
            outputs(Task::done(1).then(|n| Task::batch([Task::done(n), Task::done(n + 1)]))),
            [1, 2]
        );
        assert_eq!(outputs(Task::done(1).chain(Task::done(2))), [1, 2]);
        assert_eq!(
            outputs(Task::done(Some(3)).and_then(|n| Task::done(n * 2))),
            [6]
        );
        assert_eq!(
            outputs(Task::done(None::<u8>).and_then(Task::done)),
            Vec::<u8>::new()
        );
        assert_eq!(outputs(Task::done(1).discard::<u8>()), Vec::<u8>::new());
        let run = Task::run(stream::iter([1, 2, 3]), |n| n * 2);
        assert_eq!(outputs(run), [2, 4, 6]);
    }

    /// The answer to a request comes back as an output, through `map` and `then` alike.
    #[test]
    fn an_answer_comes_back_as_an_output() {
        let task = read_clipboard().map(|text| text.map(|t| t.len()));
        let stream = task.stream.unwrap();
        let answers = futures::executor::block_on(async {
            let mut stream = stream;
            let mut out = Vec::new();
            while let Some(action) = stream.next().await {
                match action {
                    Action::Effect(Effect::ReadClipboard(sender)) => {
                        let _ = sender.send(Some("[FAKE]".into()));
                    }
                    Action::Effect(_) => unreachable!(),
                    Action::Output(o) => out.push(o),
                }
            }
            out
        });
        assert_eq!(answers, [Some(6)]);
    }

    #[test]
    fn a_channel_streams_what_is_sent_until_its_sender_ends() {
        use futures::SinkExt;
        let items = channel(4, async |mut sender| {
            for n in 1..=3 {
                let _ = sender.send(n).await;
            }
        });
        let items: Vec<u8> = futures::executor::block_on(items.collect());
        assert_eq!(items, [1, 2, 3]);
    }
}
