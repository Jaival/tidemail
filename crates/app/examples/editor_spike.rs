//! Phase 3 slice 0: throwaway spike. Is gpui-component's code `Editor` usable
//! as a prose editor for compose? (ADR 0004)
//!
//! `cargo run --release -p tidemail --example editor_spike` opens the editor
//! with 5,000 words and fake spelling squiggles for checks by hand.
//! Add `-- --bench` to type 300 keys into the middle of the text and print
//! keystroke-to-paint times (dispatch until the editor's frame is painted).

#![windows_subsystem = "windows"]

use std::{borrow::Cow, cell::RefCell, rc::Rc, time::Instant};

use std::ops::Range;

use gpui_kit::base::input::{FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, Rope};
use gpui_kit::component::input::{Editor, EditorState, Position};
use gpui_kit::*;

const WORDS: usize = 5_000;
const PARAGRAPH_WORDS: usize = 90;
const MISSPELL_EVERY: usize = 37;
const KEYS: usize = 300;

const VOCAB: &[&str] = &[
    "the",
    "meeting",
    "moved",
    "to",
    "Thursday",
    "because",
    "several",
    "people",
    "were",
    "travelling",
    "and",
    "we",
    "still",
    "need",
    "a",
    "decision",
    "on",
    "budget",
    "for",
    "next",
    "quarter",
    "please",
    "send",
    "your",
    "notes",
    "before",
    "lunch",
    "so",
    "I",
    "can",
    "merge",
    "them",
    "into",
    "one",
    "document",
    "that",
    "everyone",
    "reads",
    "carefully",
    "thanks",
    "again",
    "for",
    "the",
    "quick",
    "reply",
    "yesterday",
    "it",
    "helped",
    "more",
    "than",
];

/// 5,000 words in paragraphs; every `MISSPELL_EVERY`th word gets a typo
/// ("xq" inside it). Returns the text and the number of typos.
fn sample() -> (String, usize) {
    let mut text = String::new();
    let mut typos = 0;
    for i in 0..WORDS {
        if i > 0 && i % PARAGRAPH_WORDS == 0 {
            text.push_str(".\n\n");
        } else if i > 0 {
            text.push(' ');
        }
        let mut word = VOCAB[(i * 7 + i / 13) % VOCAB.len()].to_string();
        if i % MISSPELL_EVERY == 5 {
            word.insert_str(word.len() / 2, "xq");
            typos += 1;
        }
        text.push_str(&word);
    }
    text.push_str(".\n");
    (text, typos)
}

/// Prose highlighter: squiggles under misspelled words. Slice 5 would dim
/// Markdown markers here too.
#[derive(Default)]
struct Prose {
    text: Rope,
}

impl InputHighlighter for Prose {
    fn language(&self) -> SharedString {
        "prose".into()
    }
    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.text = text.clone();
    }
    fn styles(
        &self,
        range: &Range<usize>,
        _: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        // Squiggles from the highlighter, not editor diagnostics: diagnostics
        // draw in an unset theme colour (invisible) and are cleared on every
        // edit. Fake checker: words containing "xq".
        let slice = self.text.slice(range.clone()).to_string();
        let mut out = Vec::new();
        let mut at = range.start;
        let mut word_start = None;
        for (i, ch) in slice.char_indices().chain([(slice.len(), ' ')]) {
            if ch.is_alphanumeric() {
                word_start.get_or_insert(i);
                continue;
            }
            let Some(ws) = word_start.take() else {
                continue;
            };
            if slice[ws..i].contains("xq") {
                let (a, b) = (range.start + ws, range.start + i);
                if a > at {
                    out.push((at..a, HighlightStyle::default()));
                }
                out.push((
                    a..b,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            color: Some(rgb(0xd03030).into()),
                            thickness: px(1.),
                            wavy: true,
                        }),
                        ..Default::default()
                    },
                ));
                at = b;
            }
        }
        if at < range.end {
            out.push((at..range.end, HighlightStyle::default()));
        }
        out
    }
    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

struct Spike {
    editor: Entity<EditorState>,
    /// Set when a key is dispatched; read when the frame after it paints.
    pressed: Rc<RefCell<Option<Instant>>>,
    samples: Rc<RefCell<Vec<f64>>>,
}

impl Render for Spike {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let pressed = self.pressed.clone();
        let samples = self.samples.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1b1f23))
            .p_4()
            .child(
                Editor::new(&self.editor)
                    .font_family("Inter")
                    .text_size(px(15.))
                    .h_full(),
            )
            // Painted after the editor: dispatch -> here = keystroke to paint.
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, _| {
                        if let Some(t) = pressed.borrow_mut().take() {
                            samples
                                .borrow_mut()
                                .push(t.elapsed().as_secs_f64() * 1000.0);
                        }
                    },
                )
                .size_0(),
            )
    }
}

fn main() {
    let bench = std::env::args().any(|a| a == "--bench");
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/inter/Inter-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/inter/Inter-Bold.ttf").as_slice()),
            ])
            .expect("fonts");

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(900.), px(800.)), cx)),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let (text, typos) = sample();
            let editor = cx.new(|cx| {
                let mut state = EditorState::new(window, cx)
                    .soft_wrap(true)
                    .line_number(false)
                    .folding(false)
                    .indent_guides(false)
                    .auto_close(false)
                    .smart_indent(false)
                    .language("prose")
                    .default_value(text);
                // Without the tree-sitter/syntect features there is no highlighter;
                // install ours.
                state.set_highlighter_factory(
                    Rc::new(
                        |_: &str| Some(Box::new(Prose::default()) as Box<dyn InputHighlighter>),
                    ),
                    cx,
                );
                state
            });
            eprintln!(
                "spike: {} words, {} squiggles, text length {}",
                WORDS,
                typos,
                editor.read(cx).value().len()
            );
            let spike = cx.new(|_| Spike {
                editor: editor.clone(),
                pressed: Rc::default(),
                samples: Rc::default(),
            });
            // Caret in the middle of the text, inside a long wrapped paragraph.
            let mid = (WORDS / PARAGRAPH_WORDS) as u32;
            editor.update(cx, |e, cx| {
                e.set_cursor_position(Position::new(mid, 40), window, cx)
            });
            if bench {
                let spike = spike.clone();
                window.on_next_frame(move |window, cx| type_keys(spike, 0, window, cx));
            }
            spike
        })
        .expect("window");
    });
}

fn type_keys(spike: Entity<Spike>, step: usize, window: &mut Window, cx: &mut App) {
    if step == KEYS {
        let mut s = spike.read(cx).samples.borrow().clone();
        s.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f64| s[((s.len() as f64 * p) as usize).min(s.len() - 1)];
        let len = spike.read(cx).editor.read(cx).value().len();
        println!("text length after typing: {len}");
        println!(
            "keystroke to paint: n={} p50={:.2} ms p95={:.2} ms max={:.2} ms",
            s.len(),
            pct(0.5),
            pct(0.95),
            s[s.len() - 1]
        );
        cx.quit();
        return;
    }
    // Words of 5 letters, a space, and a backspace now and then.
    let key = match step % 8 {
        5 => "space",
        7 => "backspace",
        _ => "a",
    };
    *spike.read(cx).pressed.borrow_mut() = Some(Instant::now());
    window.dispatch_keystroke(Keystroke::parse(key).expect("key"), cx);
    window.on_next_frame(move |window, cx| {
        // Next frame starts after the edit frame painted; type the next key.
        window.on_next_frame(move |window, cx| type_keys(spike, step + 1, window, cx));
        window.refresh();
        let _ = cx;
    });
}
