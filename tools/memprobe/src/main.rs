// Measures heap (counting allocator) and RSS at each stage of a CPU-rendered Soft UI frame.
use std::alloc::{GlobalAlloc, Layout as L, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Count;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: L) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        let c = CUR.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(c, Relaxed);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: L) {
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Relaxed);
    }
}
#[global_allocator]
static A: Count = Count;

fn rss_kb() -> usize {
    std::fs::read_to_string("/proc/self/status").unwrap().lines()
        .find(|l| l.starts_with("VmRSS")).unwrap()
        .split_whitespace().nth(1).unwrap().parse().unwrap()
}
fn report(stage: &str) {
    println!("{stage:<44} heap={:>7.2} MiB  peak={:>7.2} MiB  rss={:>7.2} MiB",
        CUR.load(Relaxed) as f64 / 1048576.0, PEAK.load(Relaxed) as f64 / 1048576.0, rss_kb() as f64 / 1024.0);
}

use vello_cpu::kurbo::{Affine, Rect, RoundedRect, Shape};
use vello_cpu::peniko::color::{AlphaColor, Srgb};
use vello_cpu::{Pixmap, RenderContext, Resources};

fn main() {
    let (w, h): (u16, u16) = std::env::args().nth(1).map(|s| {
        let (a, b) = s.split_once('x').unwrap(); (a.parse().unwrap(), b.parse().unwrap())
    }).unwrap_or((1280, 800));
    report("start");

    let mut font_cx = parley::FontContext::new();
    let mut layout_cx: parley::LayoutContext<()> = parley::LayoutContext::new();
    report("parley FontContext (system font index)");

    let text = "Kiesel UI – Soft UI für den Desktop. Größe, Gewicht und Farbe kommen aus den Tokens; \
                die Schrift aus dem System. Ärger mit IME? Nein: 日本語 와 한국어 und العربية.";
    let mut layouts = Vec::new();
    for i in 0..40 {
        let mut b = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
        b.push_default(parley::StyleProperty::FontSize(13.0 + (i % 4) as f32));
        let mut lay = b.build(text);
        lay.break_all_lines(Some(360.0));
        lay.align(parley::Alignment::Start, Default::default());
        layouts.push(lay);
    }
    report("40 paragraphs shaped (incl. CJK/Arabic fallback)");

    let mut pix = Pixmap::new(w, h);
    report(&format!("pixmap {w}x{h}"));
    let mut ctx = RenderContext::new(w, h);
    let mut res = Resources::new();
    report("vello_cpu RenderContext + Resources");

    let bg = AlphaColor::<Srgb>::from_rgb8(0xe4, 0xe8, 0xee);
    let hi = AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 200);
    let lo = AlphaColor::<Srgb>::from_rgba8(0xa3, 0xb1, 0xc6, 180);
    let mut frame = |ctx: &mut RenderContext, res: &mut Resources, pix: &mut Pixmap| {
        ctx.reset();
        ctx.set_paint(bg);
        ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
        // 120 raised cards/buttons: two soft shadows + face, like --ksl-raised
        for i in 0..120 {
            let x = 20.0 + (i % 12) as f64 * 100.0;
            let y = 20.0 + (i / 12) as f64 * 75.0;
            let r = Rect::new(x, y, x + 84.0, y + 56.0);
            ctx.set_paint(lo);
            ctx.fill_blurred_rounded_rect(&(r + vello_cpu::kurbo::Vec2::new(4.0, 4.0)), 14.0, 6.0, false);
            ctx.set_paint(hi);
            ctx.fill_blurred_rounded_rect(&(r - vello_cpu::kurbo::Vec2::new(4.0, 4.0)), 14.0, 6.0, false);
            ctx.set_paint(bg);
            ctx.fill_path(&RoundedRect::from_rect(r, 14.0).to_path(0.1));
        }
        for (i, lay) in layouts.iter().enumerate() {
            let ox = 20.0 + (i % 3) as f64 * 400.0;
            let oy = 20.0 + (i / 3) as f64 * 58.0;
            ctx.set_transform(Affine::translate((ox, oy)));
            ctx.set_paint(AlphaColor::<Srgb>::from_rgb8(0x33, 0x3a, 0x45));
            for line in lay.lines() {
                for item in line.items() {
                    if let parley::PositionedLayoutItem::GlyphRun(gr) = item {
                        let run = gr.run();
                        let glyphs = gr.positioned_glyphs().map(|g| vello_cpu::Glyph { id: g.id, x: g.x, y: g.y });
                        let _ = ctx.glyph_run(res, run.font()).font_size(run.font_size()).fill_glyphs(glyphs);
                    }
                }
            }
            ctx.reset_transform();
        }
        ctx.flush();
        ctx.render(&mut *pix, res);
    };
    let t = std::time::Instant::now();
    frame(&mut ctx, &mut res, &mut pix);
    report(&format!("first frame ({:.1} ms)", t.elapsed().as_secs_f64() * 1e3));
    let t = std::time::Instant::now();
    for _ in 0..10 { frame(&mut ctx, &mut res, &mut pix); }
    report(&format!("after 10 more frames ({:.1} ms/frame)", t.elapsed().as_secs_f64() * 1e2));
    if std::env::var("PNG").is_ok() { std::fs::write("frame.png", pix.into_png().unwrap()).unwrap(); }
}
