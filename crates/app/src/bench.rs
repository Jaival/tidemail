//! `--bench <report.json>`: measure cold launch, j/k open-to-paint, list
//! scrolling and open compose to caret paint in the running app, write a
//! JSON report and quit.

use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Instant};

use gpui_kit::*;
use serde::Serialize;

use tidemail_db::types::OutgoingMail;

use crate::features::shell::Shell;

const JK_STEPS: usize = 100;
const SCROLL_FRAMES: usize = 600;
const COMPOSE_STEPS: usize = 20;
/// A frame interval above this counts as dropped (1.5 × 60 Hz).
const DROPPED_MS: f64 = 25.0;
/// Give up waiting for the reader after this many frames.
const MAX_WAIT_FRAMES: usize = 120;

#[derive(Serialize, Default)]
pub struct Report {
    pub cold_launch_ms: f64,
    /// j/k press until the reader shows the thread, per step.
    pub jk_ms: Vec<f64>,
    pub scroll_frames: usize,
    pub scroll_dropped: usize,
    pub scroll_blank: usize,
    pub scroll_max_frame_ms: f64,
    /// Open compose until the form with its caret is painted, per step.
    pub compose_ms: Vec<f64>,
}

struct Run {
    shell: Entity<Shell>,
    path: PathBuf,
    report: Report,
    step: usize,
    waited: usize,
    pressed: Instant,
    last_frame: Instant,
}

type Shared = Rc<RefCell<Run>>;

pub fn start(
    shell: Entity<Shell>,
    path: PathBuf,
    cold_launch_ms: f64,
    window: &mut Window,
    cx: &mut App,
) {
    let run = Rc::new(RefCell::new(Run {
        shell,
        path,
        report: Report {
            cold_launch_ms,
            ..Default::default()
        },
        step: 0,
        waited: 0,
        pressed: Instant::now(),
        last_frame: Instant::now(),
    }));
    // Without open-on-move (Command), j/k open only while reading: open first.
    let list = run.borrow().shell.read(cx).list.clone();
    if !run
        .borrow()
        .shell
        .read(cx)
        .state
        .read(cx)
        .spec()
        .open_on_move
    {
        list.update(cx, |l, cx| l.open_current(cx));
    }
    press(run, window);
}

fn press(run: Shared, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        {
            let mut r = run.borrow_mut();
            let list = r.shell.read(cx).list.clone();
            let forward = r.step < JK_STEPS / 2;
            list.update(cx, |l, cx| match forward {
                true => l.select_next(cx),
                false => l.select_prev(cx),
            });
            r.pressed = Instant::now();
            r.waited = 0;
        }
        wait_reader(run, window);
    });
}

fn wait_reader(run: Shared, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let done = {
            let mut r = run.borrow_mut();
            let shell = r.shell.read(cx);
            let want = shell.state.read(cx).selected;
            let shown = shell.reader.read(cx).shown();
            r.waited += 1;
            if want.is_some() && shown == want || r.waited > MAX_WAIT_FRAMES {
                let ms = r.pressed.elapsed().as_secs_f64() * 1000.0;
                r.report.jk_ms.push(ms);
                r.step += 1;
                true
            } else {
                false
            }
        };
        window.refresh();
        if !done {
            wait_reader(run, window);
        } else if run.borrow().step < JK_STEPS {
            press(run, window);
        } else {
            // Scroll the list, not the reader (Command shows one of them).
            let state = run.borrow().shell.read(cx).state.clone();
            state.update(cx, |s, cx| s.close_reader(cx));
            let list = run.borrow().shell.read(cx).list.clone();
            list.update(cx, |l, _| {
                l.scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(0.)))
            });
            run.borrow_mut().step = 0;
            run.borrow_mut().last_frame = Instant::now();
            scroll(run, window);
        }
    });
}

fn scroll(run: Shared, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let finished = {
            let mut r = run.borrow_mut();
            let now = Instant::now();
            let frame_ms = (now - r.last_frame).as_secs_f64() * 1000.0;
            r.last_frame = now;
            if r.step > 0 {
                r.report.scroll_frames += 1;
                r.report.scroll_max_frame_ms = r.report.scroll_max_frame_ms.max(frame_ms);
                if frame_ms > DROPPED_MS {
                    r.report.scroll_dropped += 1;
                }
            }
            let list = r.shell.read(cx).list.clone();
            let blank = list.update(cx, |l, cx| {
                let handle = &l.scroll.0.borrow().base_handle;
                let viewport = f32::from(handle.bounds().size.height);
                let top = -f32::from(handle.offset().y);
                // Scroll step: 3 rows of the active preset.
                let row_height = l.row_height(cx);
                let loaded = l.rows.len() as f32 * row_height;
                let blank = top + viewport > loaded + 0.5 && l.has_more();
                handle.set_offset(point(px(0.), -px(top + 3.0 * row_height)));
                cx.notify();
                blank
            });
            if blank && r.step > 0 {
                r.report.scroll_blank += 1;
            }
            r.step += 1;
            r.step > SCROLL_FRAMES
        };
        if finished {
            run.borrow_mut().step = 0;
            open_compose(run, window);
        } else {
            scroll(run, window);
        }
    });
}

fn open_compose(run: Shared, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        if run.borrow().step == COMPOSE_STEPS {
            finish(&run.borrow(), cx);
            return;
        }
        let shell = run.borrow().shell.clone();
        shell.update(cx, |s, cx| s.open_compose(OutgoingMail::default(), window, cx));
        let mut r = run.borrow_mut();
        r.pressed = Instant::now();
        r.waited = 0;
        drop(r);
        wait_compose(run, window);
    });
}

fn wait_compose(run: Shared, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let shell = run.borrow().shell.clone();
        let painted = shell.read(cx).compose().and_then(|c| c.read(cx).painted());
        let done = {
            let mut r = run.borrow_mut();
            r.waited += 1;
            if let Some(t) = painted {
                let ms = (t - r.pressed).as_secs_f64() * 1000.0;
                r.report.compose_ms.push(ms);
            }
            painted.is_some() || r.waited > MAX_WAIT_FRAMES
        };
        window.refresh();
        if done {
            shell.update(cx, |s, cx| s.close_compose(window, cx));
            run.borrow_mut().step += 1;
            open_compose(run, window);
        } else {
            wait_compose(run, window);
        }
    });
}

fn finish(r: &Run, cx: &mut App) {
    match serde_json::to_vec_pretty(&r.report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&r.path, json) {
                eprintln!("tidemail: writing bench report failed: {e}");
            }
        }
        Err(e) => eprintln!("tidemail: bench report: {e}"),
    }
    cx.quit();
}
