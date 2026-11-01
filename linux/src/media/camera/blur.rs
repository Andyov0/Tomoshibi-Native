//! The background behind somebody blurred, frame by frame, before the frame
//! is sent and shown (live/blur.ts; Windows' BackgroundBlur.cs and
//! Tomoshibi.Basics/BlurMath.cs, Android's BlurMath.kt, which this follows
//! line for line, so that the three blur alike).
//!
//! The person is found by the web's own segmenter: MediaPipe's selfie model,
//! the file the web serves, as the ONNX that windows/Blur/convert.py made
//! from it and checked against MediaPipe, run here by ONNX Runtime (the ort
//! crate, linked statically). So what is kept and what is blurred is what a
//! browser keeps and blurs. The model is built into the binary: it is less
//! than half a megabyte, and a file beside an AppImage is one more thing to
//! go missing.
//!
//! Finding the person can take longer than a frame has, so it runs on a
//! thread of its own and never holds a frame up: each frame is blended
//! through the newest mask there is, and the segmenter is handed the next
//! frame whenever it is free. Until the first mask, the whole frame is
//! blurred: somebody who asked for their room to be hidden is shown nothing
//! of it while the segmenter is starting. A segmenter that will not start
//! leaves the whole frame blurred for as long as blur is on, for the same
//! reason, and says so in the log.
//!
//! Linux cameras are upright, so the rotation BlurMath handles elsewhere is
//! always none here.

use std::{
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use crate::app::log;

/// The web's radius, at a picture 720 rows high (blur.ts `RADIUS`).
pub const SIGMA_AT_720: f64 = 12.0;
/// The model's square side; the frame is stretched to it, as MediaPipe does.
pub const MODEL_SIDE: usize = 256;

static MODEL: &[u8] = include_bytes!("../../../../windows/Blur/selfie_segmenter.onnx");

/// For each pixel along one side of a plane `factor` times the grid's
/// `cells`: its cell, the next one, and how far between.
struct Taps {
    cell: Vec<usize>,
    next: Vec<usize>,
    toward: Vec<f32>,
}

impl Taps {
    fn new(count: usize, factor: usize, cells: usize) -> Taps {
        let mut taps = Taps {
            cell: Vec::with_capacity(count),
            next: Vec::with_capacity(count),
            toward: Vec::with_capacity(count),
        };
        for i in 0..count {
            let position = ((i as f32 + 0.5) / factor as f32 - 0.5).clamp(0.0, (cells - 1) as f32);
            let at = position as usize;
            taps.cell.push(at);
            taps.next.push((at + 1).min(cells - 1));
            taps.toward.push(position - at as f32);
        }
        taps
    }
}

/// The arithmetic of the blur on packed I420 planes, apart from finding the
/// person (BlurMath.cs): the frame taken down to a quarter, blurred there
/// with a Gaussian of a quarter of the radius, and laid under the frame
/// through the person's mask.
pub struct BlurMath {
    pub width: usize,
    pub height: usize,
    pub grid_width: usize,
    pub grid_height: usize,
    low_y: Vec<f32>,
    low_u: Vec<f32>,
    low_v: Vec<f32>,
    scratch: Vec<f32>,
    kernel: Vec<f32>,
    luma_x: Taps,
    luma_y: Taps,
    chroma_x: Taps,
    chroma_y: Taps,
}

impl BlurMath {
    pub fn new(width: usize, height: usize) -> BlurMath {
        let grid_width = width.div_ceil(4).max(1);
        let grid_height = height.div_ceil(4).max(1);
        let cells = grid_width * grid_height;
        BlurMath {
            width,
            height,
            grid_width,
            grid_height,
            low_y: vec![0.0; cells],
            low_u: vec![0.0; cells],
            low_v: vec![0.0; cells],
            scratch: vec![0.0; cells],
            kernel: gaussian(SIGMA_AT_720 * height as f64 / 720.0 / 4.0),
            luma_x: Taps::new(width, 4, grid_width),
            luma_y: Taps::new(height, 4, grid_height),
            chroma_x: Taps::new(width.div_ceil(2), 2, grid_width),
            chroma_y: Taps::new(height.div_ceil(2), 2, grid_height),
        }
    }

    /// Blurs the background of the planes in place. `person` is the grid's
    /// share of person in each cell, or `None` where nobody has been found
    /// yet, in which case the whole frame is blurred.
    pub fn blur(&mut self, y: &mut [u8], u: &mut [u8], v: &mut [u8], person: Option<&[f32]>) {
        let (cw, ch) = (self.width.div_ceil(2), self.height.div_ceil(2));
        let (gw, gh) = (self.grid_width, self.grid_height);
        shrink(
            y,
            self.width,
            self.height,
            4,
            gw,
            gh,
            &mut self.low_y,
            &mut self.scratch,
        );
        shrink(u, cw, ch, 2, gw, gh, &mut self.low_u, &mut self.scratch);
        shrink(v, cw, ch, 2, gw, gh, &mut self.low_v, &mut self.scratch);
        smooth(&mut self.low_y, gw, gh, &self.kernel, &mut self.scratch);
        smooth(&mut self.low_u, gw, gh, &self.kernel, &mut self.scratch);
        smooth(&mut self.low_v, gw, gh, &self.kernel, &mut self.scratch);
        blend(
            y,
            self.width,
            self.height,
            &self.luma_x,
            &self.luma_y,
            gw,
            &self.low_y,
            person,
        );
        blend(
            u,
            cw,
            ch,
            &self.chroma_x,
            &self.chroma_y,
            gw,
            &self.low_u,
            person,
        );
        blend(
            v,
            cw,
            ch,
            &self.chroma_x,
            &self.chroma_y,
            gw,
            &self.low_v,
            person,
        );
    }
}

/// A plane averaged down onto the grid, `factor` of its pixels to a cell
/// each way.
#[allow(clippy::too_many_arguments)]
fn shrink(
    plane: &[u8],
    width: usize,
    height: usize,
    factor: usize,
    gw: usize,
    gh: usize,
    output: &mut [f32],
    counts: &mut [f32],
) {
    output.iter_mut().for_each(|o| *o = 0.0);
    counts.iter_mut().for_each(|c| *c = 0.0);
    for row in 0..height {
        let cell = (row / factor).min(gh - 1) * gw;
        let at = row * width;
        for column in 0..width {
            let index = cell + (column / factor).min(gw - 1);
            output[index] += f32::from(plane[at + column]);
            counts[index] += 1.0;
        }
    }
    for (value, count) in output.iter_mut().zip(counts.iter()) {
        if *count > 0.0 {
            *value /= *count;
        }
    }
}

/// The Gaussian on the grid, across and then down, with the edges held.
fn smooth(plane: &mut [f32], gw: usize, gh: usize, kernel: &[f32], scratch: &mut [f32]) {
    let radius = kernel.len() / 2;
    if radius == 0 {
        return;
    }
    let clamp = |at: isize, size: usize| at.clamp(0, size as isize - 1) as usize;
    for row in 0..gh {
        let at = row * gw;
        for column in 0..gw {
            let mut sum = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                sum +=
                    weight * plane[at + clamp(column as isize + k as isize - radius as isize, gw)];
            }
            scratch[at + column] = sum;
        }
    }
    for row in 0..gh {
        for column in 0..gw {
            let mut sum = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                sum += weight
                    * scratch[clamp(row as isize + k as isize - radius as isize, gh) * gw + column];
            }
            plane[row * gw + column] = sum;
        }
    }
}

fn bilinear(grid: &[f32], r0: usize, r1: usize, c0: usize, c1: usize, fx: f32, fy: f32) -> f32 {
    let top = grid[r0 + c0] + (grid[r0 + c1] - grid[r0 + c0]) * fx;
    let bottom = grid[r1 + c0] + (grid[r1 + c1] - grid[r1 + c0]) * fx;
    top + (bottom - top) * fy
}

/// Each pixel between itself and the blurred grid under it, by how much of a
/// person the mask says is there.
#[allow(clippy::too_many_arguments)]
fn blend(
    plane: &mut [u8],
    width: usize,
    height: usize,
    xs: &Taps,
    ys: &Taps,
    gw: usize,
    low: &[f32],
    person: Option<&[f32]>,
) {
    for row in 0..height {
        let (r0, r1, fy) = (ys.cell[row] * gw, ys.next[row] * gw, ys.toward[row]);
        let at = row * width;
        for column in 0..width {
            let (c0, c1, fx) = (xs.cell[column], xs.next[column], xs.toward[column]);
            let alpha = person.map_or(0.0, |person| bilinear(person, r0, r1, c0, c1, fx, fy));
            if alpha >= 0.996 {
                continue;
            }
            let behind = bilinear(low, r0, r1, c0, c1, fx, fy);
            let pixel = f32::from(plane[at + column]);
            plane[at + column] = (behind + alpha * (pixel - behind))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

/// A normalised Gaussian of `sigma` cells, out to three sigma.
pub fn gaussian(sigma: f64) -> Vec<f32> {
    if sigma < 0.3 {
        return vec![1.0];
    }
    let radius = (sigma * 3.0).ceil() as i64;
    let weights: Vec<f64> = (-radius..=radius)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    weights.iter().map(|w| (w / sum) as f32).collect()
}

/// The person's mask (`mask_width` by `mask_height`, upright) laid on the
/// grid of a frame `width` by `height`, its edge softened by a few cells
/// (the web blurs its mask by eight pixels at full size, `maskBlurRadius`,
/// which is two here).
pub fn person_on_grid(
    mask: &[f32],
    mask_width: usize,
    mask_height: usize,
    width: usize,
    height: usize,
    gw: usize,
    gh: usize,
) -> Vec<f32> {
    let mut output = vec![0f32; gw * gh];
    for gy in 0..gh {
        let y = (gy * 4 + 2).min(height - 1);
        for gx in 0..gw {
            let x = (gx * 4 + 2).min(width - 1);
            let mx = (x * mask_width / width).min(mask_width - 1);
            let my = (y * mask_height / height).min(mask_height - 1);
            output[gy * gw + gx] = mask[my * mask_width + mx].clamp(0.0, 1.0);
        }
    }
    // A box of five cells, twice, across and down.
    let mut scratch = vec![0f32; output.len()];
    for _ in 0..2 {
        boxed(&output, &mut scratch, gw, gh, true);
        boxed(&scratch, &mut output, gw, gh, false);
    }
    output
}

fn boxed(from: &[f32], to: &mut [f32], w: usize, h: usize, across: bool) {
    for row in 0..h {
        for column in 0..w {
            let mut sum = 0.0;
            for k in -2isize..=2 {
                let (r, c) = if across {
                    (row, (column as isize + k).clamp(0, w as isize - 1) as usize)
                } else {
                    ((row as isize + k).clamp(0, h as isize - 1) as usize, column)
                };
                sum += from[r * w + c];
            }
            to[row * w + column] = sum / 5.0;
        }
    }
}

/// One pixel's colour from its luma and chroma, BT.601 at full range
/// (Planes.cs `Rgb`).
pub fn rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let luma = f32::from(y);
    let (cb, cr) = (f32::from(u) - 128.0, f32::from(v) - 128.0);
    let clamp = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    (
        clamp(luma + 1.402 * cr),
        clamp(luma - 0.344_136 * cb - 0.714_136 * cr),
        clamp(luma + 1.772 * cb),
    )
}

/// The frame stretched to the model's square, as RGB floats from 0 to 1
/// (BlurMath.cs `ModelInput`): MediaPipe stretches a picture that is not
/// square rather than bordering it, and convert.py measured both.
pub fn model_input(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    width: usize,
    height: usize,
    side: usize,
    output: &mut [f32],
) {
    let cw = width.div_ceil(2);
    let mut at = 0;
    for sy in 0..side {
        let row = (((sy as f64 + 0.5) * height as f64 / side as f64) as usize).min(height - 1);
        for sx in 0..side {
            let column = (((sx as f64 + 0.5) * width as f64 / side as f64) as usize).min(width - 1);
            let c = (row / 2) * cw + column / 2;
            let (r, g, b) = rgb(y[row * width + column], u[c], v[c]);
            output[at] = f32::from(r) / 255.0;
            output[at + 1] = f32::from(g) / 255.0;
            output[at + 2] = f32::from(b) / 255.0;
            at += 3;
        }
    }
}

/// The selfie model in ONNX Runtime, on the processor: small, a few
/// milliseconds a frame, and the graphics card is the thing a call is
/// already busy with.
pub struct Segmenter {
    session: ort::session::Session,
}

fn environment() -> Result<ort::environment::Environment, String> {
    static ENVIRONMENT: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
    ENVIRONMENT
        .get_or_init(|| {
            ort::init()
                .with_name("tomoshibi")
                .build()
                .map_err(|e| e.to_string())
        })
        .clone()
}

impl Segmenter {
    pub fn load() -> Result<Segmenter, String> {
        let environment = environment()?;
        let session = ort::session::Session::builder(&environment)
            .map_err(|e| e.to_string())?
            .with_intra_threads(2)
            .map_err(|e| e.to_string())?
            .commit_from_memory(MODEL)
            .map_err(|e| e.to_string())?;
        Ok(Segmenter { session })
    }

    /// The person's share of each pixel of the model's square, from its RGB
    /// input (NHWC, `MODEL_SIDE` square).
    pub fn run(&mut self, input: &[f32]) -> Result<Vec<f32>, String> {
        let tensor =
            ort::value::Tensor::from_array(([1usize, MODEL_SIDE, MODEL_SIDE, 3], input.to_vec()))
                .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let first = outputs.values().next().ok_or("no output")?;
        let (_, mask) = first
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if mask.len() < MODEL_SIDE * MODEL_SIDE {
            return Err(format!("a mask of {} values", mask.len()));
        }
        Ok(mask[..MODEL_SIDE * MODEL_SIDE].to_vec())
    }
}

/// The newest mask, on the grid of the frame size it was found in.
struct Person {
    width: usize,
    height: usize,
    grid: Vec<f32>,
}

#[derive(Default)]
struct Work {
    /// A frame is with the segmenter.
    busy: bool,
    closed: bool,
    failed: bool,
    input: Vec<f32>,
    size: (usize, usize, usize, usize),
    person: Option<Arc<Person>>,
}

/// The blur of one camera: the math on the capture thread, the segmenter
/// on a thread of its own.
pub struct Blur {
    work: Arc<(Mutex<Work>, Condvar)>,
    math: Option<BlurMath>,
    frames: u64,
    spent: Duration,
}

impl Blur {
    pub fn new() -> Blur {
        let work: Arc<(Mutex<Work>, Condvar)> =
            Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let worker = work.clone();
        let spawned = thread::Builder::new()
            .name("tomoshibi segment".into())
            .spawn(move || segment(worker));
        if let Err(fault) = spawned {
            log(&format!(
                "blur: the segmenter's thread did not start: {fault}"
            ));
            work.0.lock().unwrap_or_else(|p| p.into_inner()).failed = true;
        }
        Blur {
            work,
            math: None,
            frames: 0,
            spent: Duration::ZERO,
        }
    }

    /// Blurs what is behind the person in the planes, in place.
    pub fn process(
        &mut self,
        y: &mut [u8],
        u: &mut [u8],
        v: &mut [u8],
        width: usize,
        height: usize,
    ) {
        let began = Instant::now();
        if !matches!(&self.math, Some(math) if math.width == width && math.height == height) {
            self.math = Some(BlurMath::new(width, height));
        }
        let Some(math) = self.math.as_mut() else {
            return;
        };
        let found = {
            let (lock, woken) = &*self.work;
            let mut work = lock.lock().unwrap_or_else(|p| p.into_inner());
            if !work.busy && !work.failed && !work.closed {
                work.input.resize(MODEL_SIDE * MODEL_SIDE * 3, 0.0);
                model_input(y, u, v, width, height, MODEL_SIDE, &mut work.input);
                work.size = (width, height, math.grid_width, math.grid_height);
                work.busy = true;
                woken.notify_one();
            }
            work.person
                .clone()
                .filter(|p| p.width == width && p.height == height)
        };
        math.blur(y, u, v, found.as_ref().map(|p| p.grid.as_slice()));
        self.spent += began.elapsed();
        self.frames += 1;
        if self.frames.is_multiple_of(150) {
            log(&format!(
                "blur: {:.1} ms a frame on the capture thread, {}",
                self.spent.as_secs_f64() * 1000.0 / 150.0,
                if found.is_some() {
                    "a person found"
                } else {
                    "nobody found yet"
                }
            ));
            self.spent = Duration::ZERO;
        }
    }
}

impl Default for Blur {
    fn default() -> Self {
        Blur::new()
    }
}

impl Drop for Blur {
    fn drop(&mut self) {
        let (lock, woken) = &*self.work;
        let mut work = lock.lock().unwrap_or_else(|p| p.into_inner());
        work.closed = true;
        work.person = None;
        woken.notify_all();
    }
}

/// The segmenter's thread: loads the model the first time it is given a
/// frame, then finds the person in each frame it is handed.
fn segment(work: Arc<(Mutex<Work>, Condvar)>) {
    let (lock, woken) = &*work;
    let mut segmenter: Option<Segmenter> = None;
    loop {
        let (input, size) = {
            let mut held = lock.lock().unwrap_or_else(|p| p.into_inner());
            while !held.closed && !held.busy {
                held = woken.wait(held).unwrap_or_else(|p| p.into_inner());
            }
            if held.closed {
                return;
            }
            (held.input.clone(), held.size)
        };
        if segmenter.is_none() {
            match Segmenter::load() {
                Ok(loaded) => {
                    log("blur: the segmenter is ready");
                    segmenter = Some(loaded);
                }
                Err(fault) => {
                    log(&format!("blur: the segmenter would not start ({fault}); the whole picture stays blurred"));
                    let mut held = lock.lock().unwrap_or_else(|p| p.into_inner());
                    held.failed = true;
                    held.busy = false;
                    return;
                }
            }
        }
        let mask = segmenter.as_mut().map(|s| s.run(&input));
        let mut held = lock.lock().unwrap_or_else(|p| p.into_inner());
        match mask {
            Some(Ok(mask)) if !held.closed => {
                let (width, height, gw, gh) = size;
                let grid = person_on_grid(&mask, MODEL_SIDE, MODEL_SIDE, width, height, gw, gh);
                held.person = Some(Arc::new(Person {
                    width,
                    height,
                    grid,
                }));
            }
            Some(Err(fault)) => log(&format!("blur: a frame was not segmented: {fault}")),
            _ => {}
        }
        held.busy = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernel_is_normalised_and_as_wide_as_three_sigma() {
        let kernel = gaussian(3.0);
        assert_eq!(kernel.len(), 19);
        assert!((kernel.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(gaussian(0.2), vec![1.0]);
        // 720 rows: the web's 12 at a quarter is 3 cells.
        assert_eq!(BlurMath::new(1280, 720).kernel.len(), 19);
    }

    #[test]
    fn a_flat_picture_stays_flat_and_a_person_is_kept() {
        let (w, h) = (64, 48);
        let mut math = BlurMath::new(w, h);
        let mut y = vec![100u8; w * h];
        let mut u = vec![120u8; w * h / 4];
        let mut v = vec![130u8; w * h / 4];
        math.blur(&mut y, &mut u, &mut v, None);
        assert!(
            y.iter().all(|&p| p == 100)
                && u.iter().all(|&p| p == 120)
                && v.iter().all(|&p| p == 130)
        );

        // A sharp edge is softened where nobody is, and kept where the mask
        // says somebody is.
        let edge: Vec<u8> = (0..w * h)
            .map(|i| if i % w < w / 2 { 0 } else { 255 })
            .collect();
        let mut blurred = edge.clone();
        math.blur(&mut blurred, &mut u, &mut v, None);
        assert!(
            blurred[10 * w + w / 2] > 20 && blurred[10 * w + w / 2 - 1] < 235,
            "the edge was not softened"
        );
        let person = vec![1.0f32; math.grid_width * math.grid_height];
        let mut kept = edge.clone();
        math.blur(&mut kept, &mut u, &mut v, Some(&person));
        assert_eq!(kept, edge);
    }

    #[test]
    fn the_model_sees_the_colours_of_the_frame() {
        // A red frame (full-range BT.601) is red to the model.
        let (w, h) = (32, 16);
        let y = vec![76u8; w * h];
        let u = vec![85u8; w * h / 4];
        let v = vec![255u8; w * h / 4];
        let mut input = vec![0f32; 8 * 8 * 3];
        model_input(&y, &u, &v, w, h, 8, &mut input);
        assert!(
            input[0] > 0.95 && input[1] < 0.05 && input[2] < 0.05,
            "{:?}",
            &input[..3]
        );
    }

    #[test]
    fn the_mask_lands_on_the_grid_where_it_was_found() {
        // Somebody in the left half of the model's square is in the left
        // half of the grid, with a softened edge between.
        let mask: Vec<f32> = (0..MODEL_SIDE * MODEL_SIDE)
            .map(|i| {
                if i % MODEL_SIDE < MODEL_SIDE / 2 {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let math = BlurMath::new(1280, 720);
        let grid = person_on_grid(
            &mask,
            MODEL_SIDE,
            MODEL_SIDE,
            1280,
            720,
            math.grid_width,
            math.grid_height,
        );
        let row = 90 * math.grid_width;
        assert!(grid[row + 10] > 0.99 && grid[row + 300] < 0.01);
        let middle = grid[row + 160];
        assert!(middle > 0.2 && middle < 0.8, "edge {middle}");
    }

    /// The model itself, through ONNX Runtime: a frame of nothing but a
    /// flat wall has nobody in it, and the model answers with a mask of the
    /// model's square. This is what shows the runtime is linked and the
    /// model loads; a person is found in a picture only by the check below
    /// the camera's code, which needs one.
    #[test]
    fn the_segmenter_loads_and_finds_nobody_in_a_wall() {
        let mut segmenter = Segmenter::load().expect("the segmenter loads");
        let wall = vec![0.5f32; MODEL_SIDE * MODEL_SIDE * 3];
        let mask = segmenter.run(&wall).expect("the model runs");
        assert_eq!(mask.len(), MODEL_SIDE * MODEL_SIDE);
        let mean = mask.iter().sum::<f32>() / mask.len() as f32;
        assert!(mean < 0.2, "a grey wall is {mean} person");
    }
}
