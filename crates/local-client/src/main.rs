//! `nitro-local-client`: the local client's window.
//!
//! A thin shell: it shows [`LocalClient::frame`] in a fixed-size window,
//! forwards left clicks to [`LocalClient::click`] and steps the game on the
//! client's timer. `--snapshot` renders the same frame to a PNG without a
//! window.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use nitro_local_client::{APP_ID, ClientConfig, LAYOUT, LocalClient, Next, TITLE};
use nitro_simulator::{SeatStrategy, Structure, TrivialBot};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Parser)]
#[command(version, about = "Play an Expresso Nitro against two simulator bots")]
struct Args {
    /// Random seed: the same seed deals the same cards to the same
    /// decisions. Defaults to the clock.
    #[arg(long)]
    seed: Option<u64>,
    /// Bots of seats 1 and 2: always-all-in, always-fold or random.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "BOT,BOT",
        default_value = "random,random"
    )]
    bots: Vec<TrivialBot>,
    /// Pause after each bot action, in milliseconds.
    #[arg(long, default_value_t = 700)]
    bot_delay_ms: u64,
    /// Pause on a finished hand, in milliseconds.
    #[arg(long, default_value_t = 2_500)]
    hand_over_delay_ms: u64,
    /// Render the table at the hero's first decision to this PNG and exit,
    /// without opening a window.
    #[arg(long, value_name = "PNG")]
    snapshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let [first, second] = args.bots[..] else {
        eprintln!("error: --bots takes two bots, got {}", args.bots.len());
        return ExitCode::from(2);
    };
    let seed = args.seed.unwrap_or_else(|| {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        now.map_or(0, |d| d.as_nanos() as u64)
    });
    eprintln!("seed {seed}");
    let mut client = LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed,
        bots: [first, second].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::from_millis(args.bot_delay_ms),
        hand_over_delay: Duration::from_millis(args.hand_over_delay_ms),
    });

    if let Some(path) = args.snapshot {
        while let Next::After(_) = client.advance() {}
        return match client.frame().save_png(&path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: cannot write {}: {err}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let result = EventLoop::new().map(|event_loop| {
        let mut app = App {
            client,
            window: None,
            next_step: Some(Instant::now()),
            cursor: None,
        };
        event_loop.run_app(&mut app)
    });
    match result {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(err)) | Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

struct Shown {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    client: LocalClient,
    window: Option<Shown>,
    /// When to step the game next; `None` while waiting for a click or once
    /// the game is over.
    next_step: Option<Instant>,
    /// Pointer position in frame pixels.
    cursor: Option<(u32, u32)>,
}

impl App {
    fn schedule(&mut self, next: Next) {
        self.next_step = match next {
            Next::After(delay) => Some(Instant::now() + delay),
            Next::WaitForClick | Next::GameOver => None,
        };
        if let Some(shown) = &self.window {
            shown.window.request_redraw();
        }
    }

    fn open(&self, event_loop: &ActiveEventLoop) -> Result<Shown, String> {
        let size = PhysicalSize::new(LAYOUT.window.width, LAYOUT.window.height);
        let attributes = Window::default_attributes()
            .with_title(TITLE)
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false);
        // The Wayland app_id and the X11 class: what window rules match on.
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes, APP_ID, APP_ID,
        );
        let attributes =
            winit::platform::x11::WindowAttributesExtX11::with_name(attributes, APP_ID, APP_ID);
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| e.to_string())?,
        );
        let context = Context::new(Rc::clone(&window)).map_err(|e| e.to_string())?;
        let surface = Surface::new(&context, Rc::clone(&window)).map_err(|e| e.to_string())?;
        Ok(Shown { window, surface })
    }

    fn draw(&mut self) -> Result<(), String> {
        let Some(shown) = &mut self.window else {
            return Ok(());
        };
        let size = shown.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        shown
            .surface
            .resize(width, height)
            .map_err(|e| e.to_string())?;
        let frame = self.client.frame();
        let mut buffer = shown.surface.buffer_mut().map_err(|e| e.to_string())?;
        // The frame is copied pixel for pixel, never scaled: a window of
        // another size shows it cropped or with a black margin.
        for y in 0..size.height {
            for x in 0..size.width {
                let pixel = if x < frame.width() && y < frame.height() {
                    let [r, g, b, _] = frame.pixel(x, y);
                    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
                } else {
                    0
                };
                buffer[(y * size.width + x) as usize] = pixel;
            }
        }
        buffer.present().map_err(|e| e.to_string())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match self.open(event_loop) {
            Ok(shown) => self.window = Some(shown),
            Err(err) => {
                eprintln!("error: cannot open the window: {err}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.draw() {
                    eprintln!("error: cannot draw: {err}");
                    event_loop.exit();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x >= 0.0 && position.y >= 0.0)
                    .then_some((position.x as u32, position.y as u32));
            }
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(next) = self.cursor.and_then(|(x, y)| self.client.click(x, y)) {
                    self.schedule(next);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        while let Some(at) = self.next_step
            && at <= Instant::now()
        {
            let next = self.client.advance();
            self.schedule(next);
        }
        event_loop.set_control_flow(match self.next_step {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}
