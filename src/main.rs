extern crate rand;
extern crate tui;
use futures::future::{AbortHandle, Abortable};
use rand::Rng;
use std::convert::TryInto;
use std::io;
use std::time;
use std::time::Instant;
use termion::event::{Event, Key, MouseEvent};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::delay_for;
use tui::backend::{Backend, TermionBackend};
use tui::layout::Alignment;
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::Terminal;

enum ReactionTime {
    NoResult,
    HasResult(u16),
    FalseStart,
    Waiting,
    Ready,
}

fn draw<B: Backend>(terminal: &mut Terminal<B>, result: &ReactionTime) -> Result<(), io::Error> {
    terminal.draw(|mut f| {
        let size = f.size();
        let (text, bg) = match result {
            ReactionTime::NoResult => (
                vec![
                    Text::raw("Press a key to get started\n"),
                    Text::styled(
                        "The terminal with clear, wait for it to flash and press space or click\n",
                        Style::default().fg(Color::Red),
                    ),
                ],
                Color::Black,
            ),
            ReactionTime::FalseStart => (
                vec![
                    Text::raw("Not too fast!\n"),
                    Text::styled(
                        "That was a false start!\n",
                        Style::default().fg(Color::White),
                    ),
                ],
                Color::Red,
            ),

            ReactionTime::Waiting => (
                vec![
                    Text::raw("Press a key when the background turns green!\n"),
                    Text::styled("!\n", Style::default().fg(Color::Red)),
                ],
                Color::Blue,
            ),
            ReactionTime::HasResult(ms) => (
                vec![
                    Text::raw("Your reaction time was:\n"),
                    Text::styled(format!("{}\n", ms), Style::default().fg(Color::Blue)),
                ],
                Color::Black,
            ),
            ReactionTime::Ready => (
                vec![
                    Text::raw("TIME TO CLICK!\n"),
                    Text::styled("NOW!", Style::default().fg(Color::Red)),
                ],
                Color::Green,
            ),
        };

        Paragraph::new(text.iter())
            .block(Block::default().title("Paragraph").borders(Borders::ALL))
            .style(Style::default().fg(Color::White).bg(bg))
            .alignment(Alignment::Center)
            .wrap(true)
            .render(&mut f, size);
    })
}

fn start(mut tx: mpsc::Sender<char>) -> AbortHandle {
    let mut rng = rand::thread_rng();
    let n2: u16 = rng.gen::<u16>() % 5000 + 2000;
    let sleep_duration = time::Duration::from_millis(n2.into());
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let future = Abortable::new(
        async move {
            delay_for(sleep_duration).await;
            tx.send('\x07').await.unwrap();
        },
        abort_registration,
    );
    tokio::spawn(future);
    abort_handle
}

fn read_keys(mut tx: mpsc::Sender<char>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let stdin = io::stdin();
        for evt in stdin.events() {
            match evt {
                Ok(Event::Key(Key::Char(key))) => {
                    tx.send(key).await.unwrap();
                    if key == 'q' {
                        break;
                    }
                }
                Ok(Event::Mouse(MouseEvent::Press(_, _, _))) => {
                    tx.send('\x20').await.unwrap();
                }
                _ => {}
            }
        }
    })
}

async fn run() -> Result<(), io::Error> {
    let (tx, mut rx) = mpsc::channel(20);
    let handle = read_keys(tx.clone());
    let stdout = MouseTerminal::from(io::stdout().into_raw_mode().unwrap());
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut result = ReactionTime::NoResult;
    let mut time: Instant = Instant::now();
    let mut abort_handle = None;
    loop {
        draw(&mut terminal, &result)?;
        let key = rx.recv().await;
        match key {
            Some('q') => {
                break;
            }
            Some('\x07') => {
                result = ReactionTime::Ready;
                time = Instant::now();
            }
            Some(_) => match result {
                ReactionTime::Ready => {
                    let dur = time.elapsed();
                    result = ReactionTime::HasResult(
                        dur.as_millis().try_into().unwrap_or(std::u16::MAX),
                    );
                    eprintln!("{}", dur.as_millis());
                }
                ReactionTime::NoResult | ReactionTime::HasResult(_) | ReactionTime::FalseStart => {
                    result = ReactionTime::Waiting;
                    draw(&mut terminal, &result)?;
                    let new_tx = tx.clone();
                    abort_handle = Some(start(new_tx));
                }
                ReactionTime::Waiting => {
                    if let Some(handle) = abort_handle.as_ref() {
                        handle.abort();
                    }
                    result = ReactionTime::FalseStart;
                }
            },
            _ => (),
        }
    }
    handle.await?;
    terminal.clear()
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    run().await
}
