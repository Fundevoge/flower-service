#![windows_subsystem = "windows"]

use std::error::Error;
use std::ffi::OsStr;

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, path, thread};

use chrono::TimeZone;
use chrono::{DateTime, Local, NaiveTime};
use image::{GenericImageView as _, RgbaImage};
use winapi::um::winuser::SPI_SETDESKWALLPAPER;
use winapi::um::winuser::SPIF_UPDATEINIFILE;
use winapi::um::winuser::SystemParametersInfoW;

const BASE_PATH: &str = include_str!("../base_path.txt");

fn to_path(p: &str) -> PathBuf {
    Path::new(BASE_PATH).join(p)
}

fn store_last_wallpaper_change_and_idx(idx: usize) -> Option<()> {
    fs::write(
        to_path("last_wallpaper_and_idx.txt"),
        format!("{}\n{idx}", Local::now().timestamp()),
    )
    .ok()
}

fn get_last_wallpaper_change_and_idx() -> Option<(DateTime<Local>, usize)> {
    let file_string = fs::read_to_string(to_path("last_wallpaper_and_idx.txt")).unwrap();
    let (time_stamp, index) = file_string.trim().split_once("\n").unwrap();
    Some((
        Local
            .timestamp_opt(time_stamp.parse::<i64>().unwrap(), 0)
            .earliest()
            .unwrap(),
        index.parse::<usize>().unwrap(),
    ))
}

fn set_wallpaper<P: AsRef<Path>>(image_path: P) {
    let image_path: Vec<u16> = OsStr::new(image_path.as_ref())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            image_path.as_ptr() as *mut _,
            SPIF_UPDATEINIFILE,
        );
    }
}

fn apply_mask(input: image::Rgba<u8>, mask: image::Rgba<u8>) -> image::Rgba<u8> {
    let [r_mask, g_mask, b_mask, a_mask] = mask.0;
    let [r_canvas, g_canvas, b_canvas, _a_canvas] = input.0;

    // Perform alpha blending (compositing) between the mask and the canvas
    let alpha = a_mask as f32 / 255.0;
    let new_r = ((1.0 - alpha) * r_canvas as f32 + alpha * r_mask as f32) as u8;
    let new_g = ((1.0 - alpha) * g_canvas as f32 + alpha * g_mask as f32) as u8;
    let new_b = ((1.0 - alpha) * b_canvas as f32 + alpha * b_mask as f32) as u8;
    let new_a = 255; // Keep alpha at 255 for the resulting image

    image::Rgba([new_r, new_g, new_b, new_a])
}

fn modify_image<P: AsRef<path::Path>, Q: AsRef<path::Path>>(
    name: &str,
    input_image_path: P,
    output_image_path: Q,
) {
    const MARGIN: u32 = 50;
    const BOTTOM_EXTRA_MARGIN: u32 = 150;
    const CANVAS_WIDTH: u32 = 2560;
    const CANVAS_HEIGHT: u32 = 1530;
    const IMAGE_BOX_WIDTH: u32 = CANVAS_WIDTH - 2 * MARGIN;
    const IMAGE_BOX_HEIGHT: u32 = CANVAS_HEIGHT - 2 * MARGIN - BOTTOM_EXTRA_MARGIN;
    const TEXT_SIZE: u32 = 60;
    const OFF_WHITE_RGB: (u8, u8, u8) = (233, 223, 199);

    const CORNER_RADIUS: u32 = 50;

    let off_white = image::Rgba([OFF_WHITE_RGB.0, OFF_WHITE_RGB.1, OFF_WHITE_RGB.2, 255]);

    // Load the image
    let img = image::open(input_image_path).expect("Failed to open image");

    // Calculate the scaled image dimensions while maintaining aspect ratio
    let (orig_width, orig_height) = img.dimensions();
    let scale_factor = f32::min(
        IMAGE_BOX_WIDTH as f32 / orig_width as f32,
        IMAGE_BOX_HEIGHT as f32 / orig_height as f32,
    );

    let scaled_width = (orig_width as f32 * scale_factor) as u32;
    let scaled_height = (orig_height as f32 * scale_factor) as u32;

    // Resize the image
    let resized_img = image::imageops::resize(
        &img.to_rgba8(),
        scaled_width,
        scaled_height,
        image::imageops::FilterType::Lanczos3,
    );

    let mut corner_mask = RgbaImage::new(CORNER_RADIUS, CORNER_RADIUS);
    for y in 0..CORNER_RADIUS {
        for x in 0..CORNER_RADIUS {
            let alpha = (((x as f32) * (x as f32) + (y as f32) * (y as f32)).sqrt()
                - CORNER_RADIUS as f32
                + 0.5)
                .clamp(0., 1.)
                * 255.;
            corner_mask.put_pixel(
                x,
                y,
                image::Rgba([
                    OFF_WHITE_RGB.0,
                    OFF_WHITE_RGB.1,
                    OFF_WHITE_RGB.2,
                    alpha as u8,
                ]),
            );
        }
    }

    // Create an off-white canvas
    let mut canvas = RgbaImage::new(CANVAS_WIDTH, CANVAS_HEIGHT);

    for y in 0..CANVAS_HEIGHT {
        for x in 0..CANVAS_WIDTH {
            canvas.put_pixel(x, y, off_white);
        }
    }

    // Calculate the position to center the image on the canvas
    let image_x_offset = (CANVAS_WIDTH - scaled_width) / 2;
    let image_y_offset = (CANVAS_HEIGHT - scaled_height - BOTTOM_EXTRA_MARGIN) / 2;

    // Place the resized image on the canvas
    for y in 0..scaled_height {
        for x in 0..scaled_width {
            let pixel = resized_img.get_pixel(x, y);
            canvas.put_pixel(image_x_offset + x, image_y_offset + y, *pixel);
        }
    }

    // Apply Corner radius
    for y in 0..CORNER_RADIUS {
        for x in 0..CORNER_RADIUS {
            let pixel = apply_mask(
                *resized_img.get_pixel(x, y),
                *corner_mask.get_pixel(CORNER_RADIUS - (x + 1), CORNER_RADIUS - (y + 1)),
            );
            canvas.put_pixel(image_x_offset + x, image_y_offset + y, pixel);
        }
    }
    for y in 0..CORNER_RADIUS {
        for x in scaled_width - CORNER_RADIUS..scaled_width {
            let pixel = apply_mask(
                *resized_img.get_pixel(x, y),
                *corner_mask.get_pixel(x - (scaled_width - CORNER_RADIUS), CORNER_RADIUS - (y + 1)),
            );
            canvas.put_pixel(image_x_offset + x, image_y_offset + y, pixel);
        }
    }
    for y in scaled_height - CORNER_RADIUS..scaled_height {
        for x in scaled_width - CORNER_RADIUS..scaled_width {
            let pixel = apply_mask(
                *resized_img.get_pixel(x, y),
                *corner_mask.get_pixel(
                    x - (scaled_width - CORNER_RADIUS),
                    y - (scaled_height - CORNER_RADIUS),
                ),
            );
            canvas.put_pixel(image_x_offset + x, image_y_offset + y, pixel);
        }
    }
    for y in scaled_height - CORNER_RADIUS..scaled_height {
        for x in 0..CORNER_RADIUS {
            let pixel = apply_mask(
                *resized_img.get_pixel(x, y),
                *corner_mask
                    .get_pixel(CORNER_RADIUS - (x + 1), y - (scaled_height - CORNER_RADIUS)),
            );
            canvas.put_pixel(image_x_offset + x, image_y_offset + y, pixel);
        }
    }

    // Load the font
    let font_data = include_bytes!(r"../PlayfairDisplay-Regular.ttf"); // Adjust to the correct path of a TTF file
    let font = rusttype::Font::try_from_bytes(font_data).expect("Error loading font");

    // Write the filename on the canvas below the image
    let filename = std::path::Path::new(name)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let scale = rusttype::Scale {
        x: TEXT_SIZE as f32,
        y: TEXT_SIZE as f32,
    };
    let mut text_canvas = RgbaImage::new(CANVAS_WIDTH, TEXT_SIZE + 8);
    for y in 0..TEXT_SIZE + 8 {
        for x in 0..CANVAS_WIDTH {
            text_canvas.put_pixel(x, y, off_white);
        }
    }
    let mut max_x = 0;

    for glyph in font.layout(
        filename,
        scale,
        rusttype::point(0., (TEXT_SIZE + 8) as f32 / 2.),
    ) {
        if let Some(bb) = glyph.pixel_bounding_box() {
            let color = image::Rgba([0, 0, 0, 255]);
            glyph.draw(|x, y, v| {
                let x = (x as i32 + bb.min.x + 2) as u32;
                let y = (y as i32 + bb.min.y + 2) as u32;
                if v > 0.5 {
                    max_x = max_x.max(x);
                    text_canvas.put_pixel(x, y, color);
                }
            });
        }
    }

    // Place the text on the canvas
    let (text_offset_x, text_offset_y) = (
        (CANVAS_WIDTH - max_x) / 2,
        image_y_offset + scaled_height + MARGIN / 2,
    );

    for y in 0..TEXT_SIZE {
        for x in 0..max_x {
            let pixel = text_canvas.get_pixel(x, y);
            canvas.put_pixel(text_offset_x + x, text_offset_y + y, *pixel);
        }
    }

    // Save the result to the file
    canvas.save(output_image_path).unwrap();
}

fn main() -> Result<(), Box<dyn Error>> {
    let permutated_indices: Vec<usize> =
        fs::read_to_string(to_path("wiki_flower_permutation.txt"))?
            .split(", ")
            .flat_map(|num| num.parse())
            .collect();

    let mut image_file_names: Vec<String> = fs::read_dir(to_path("wiki_flowers"))?
        .flatten()
        .flat_map(|entry| entry.file_name().into_string())
        .collect();
    image_file_names.sort();

    let (last_timestamp, mut file_idx) =
        get_last_wallpaper_change_and_idx().ok_or("Error reading persistent storage")?;

    if last_timestamp.date_naive() == Local::now().date_naive() {
        thread::sleep(Duration::from_secs(
            NaiveTime::from_hms_opt(23, 59, 59)
                .unwrap()
                .signed_duration_since(Local::now().time())
                .num_seconds()
                .unsigned_abs(),
        ));
    }
    let output_file_path = to_path("flower_of_today.png");
    loop {
        let current_file_name = &image_file_names[permutated_indices[file_idx]];

        modify_image(
            current_file_name
                .trim_end_matches(".jpg")
                .trim_end_matches(".JPG")
                .trim_end_matches(".png"),
            to_path("wiki_flowers").join(current_file_name),
            output_file_path.clone(),
        );

        set_wallpaper(output_file_path.clone());
        file_idx = (file_idx + 1) % image_file_names.len();
        store_last_wallpaper_change_and_idx(file_idx);

        thread::sleep(Duration::from_secs(
            NaiveTime::from_hms_opt(23, 59, 59)
                .unwrap()
                .signed_duration_since(Local::now().time())
                .num_seconds()
                .unsigned_abs(),
        ));
    }
}
