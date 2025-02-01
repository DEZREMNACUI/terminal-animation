/*
   This file is part of term-video.

   term-video is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.

   term-video is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with term-video.  If not, see <https://www.gnu.org/licenses/>.
*/

use clap::Parser;
use image::{io::Reader, GenericImageView, Pixel};
use std::{
    fs,
    io::Write,
    process::{exit, Command, Stdio},
    str::FromStr,
    thread,
    time::Duration,
};
use walkdir::WalkDir;

/*
const CHARS: [char; 13] = [
    ' ', '.', ',', '-', '~', ':', ';', '=', '!', '*', '#', '$', '@',
];
 */

#[derive(Parser)]
#[command(version = "0.1.0", author = "Pascal Puffke <pascal@pascalpuffke.de>")]
struct Opts {
    #[arg(
        short,
        long,
        default_value = "split_frames",
        help = "Where to save temporary frame data"
    )]
    cache: String,
    #[arg(
        short,
        long,
        help = "Input video file, can be any format as long as it's supported by ffmpeg."
    )]
    input: String,
    #[arg(
        short,
        long,
        help = "Horizontal playback resolution [default: current terminal rows]"
    )]
    width: Option<u32>,
    #[arg(
        short,
        long,
        help = "Vertical playback resolution [default: current terminal columns]"
    )]
    height: Option<u32>,
    #[arg(
        short,
        long,
        help = "Playback frame rate [default: input video FPS, or 30 should ffprobe fail]"
    )]
    fps: Option<u32>,
}

fn main() {
    let opts = Opts::parse();
    let term_dim = term_size::dimensions().unwrap_or((80, 24));
    let w = opts.width.unwrap_or(term_dim.0 as u32);
    let h = opts.height.unwrap_or(term_dim.1 as u32);
    let fps = opts
        .fps
        .unwrap_or(get_frame_rate(&opts.input).unwrap_or(30));

    make_dir(&opts.cache);
    split_and_resize_frames(&opts.input, &opts.cache, w, h);
    display_loop(&opts.cache, w, h, fps);

    // clean up temporary directory before exiting
    fs::remove_dir_all(&opts.cache).expect("could not delete temporary directory, enjoy the mess");
}

fn make_dir(name: &str) {
    if let Err(_) = fs::create_dir(name) {
        fs::remove_dir_all(name).expect(&format!("could not delete directory {}", name));
        fs::create_dir(name).expect(&format!("could not create directory {}", name));
    }
}

fn split_and_resize_frames(file_name: &str, cache_dir: &str, width: u32, height: u32) {
    // ffmpeg -i <file_name> -f image2 -vf scale=<w:h> <cache>/frame-%07d.png
    Command::new("ffmpeg")
        .args(vec![
            "-i",
            file_name,
            "-f",
            "image2",
            "-vf",
            &format!("scale={}:{}", width, height),
            &format!("{}/frame-%07d.png", cache_dir),
        ])
        .stdout(Stdio::null())
        .output()
        .unwrap_or_else(|e| {
            println!("Failed to execute ffmpeg - do you have it installed? {}", e);
            exit(1);
        });
}

fn get_frame_rate(video: &str) -> Option<u32> {
    let ffprobe = Command::new("ffprobe")
        .args(vec![
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            video,
        ])
        .output();

    if let Ok(out) = ffprobe {
        if let Ok(fps_str) = String::from_utf8(out.stdout) {
            if let Some((num, den)) = fps_str.trim().split_once('/') {
                if let (Ok(num), Ok(den)) = (num.parse::<f32>(), den.parse::<f32>()) {
                    return Some((num / den) as u32);
                }
            }
        }
    }

    None
}

fn display_loop(cache_dir: &str, width: u32, height: u32, frame_rate: u32) {
    let mut frame_buffer = String::with_capacity((height + (width * height)) as usize);

    // 清空屏幕并移动到左上角
    print!("\x1B[2J\x1B[H");
    // 隐藏光标
    print!("\x1B[?25l");
    // 禁用行包装
    print!("\x1B[?7l");

    let mut frame_files: Vec<_> = WalkDir::new(cache_dir)
        .into_iter()
        .skip(1)
        .map(|e| e.unwrap().path().to_owned())
        .collect();
    frame_files.sort();

    let mut display_buffer: Vec<String> = Vec::with_capacity(frame_files.len());

    // 按顺序处理每一帧
    for frame_path in frame_files {
        let frame = Reader::open(&frame_path).unwrap().decode().unwrap();

        for y in 0..height {
            for x in 0..width {
                frame_buffer.push(get_pixel_char(
                    *frame.get_pixel(x, y).to_luma().0.get(0).unwrap(),
                ))
            }
            frame_buffer.push('\n');
        }

        display_buffer.push(frame_buffer.clone());
        frame_buffer.clear();
    }

    // 显示每一帧
    for frame in &display_buffer {
        // 仅移动光标到起始位置
        print!("\x1B[H");
        // 使用单次输出
        print!("{}", frame);
        // 立即刷新输出
        std::io::stdout().flush().unwrap();
        thread::sleep(Duration::from_micros((1000000 / frame_rate) as u64));
    }

    // 恢复终端设置
    print!("\x1B[?7h"); // 重新启用行包装
    print!("\x1B[?25h"); // 显示光标
    print!("\x1B[H\x1B[2J"); // 清屏并回到开始位置
}

// TODO make this less dumb
fn get_pixel_char(luminosity: u8) -> char {
    match luminosity {
        0 => ' ',
        1..=21 => '.',
        22..=43 => ',',
        44..=65 => '-',
        66..=87 => '~',
        88..=109 => ':',
        110..=131 => ';',
        132..=153 => '=',
        154..=175 => '!',
        176..=197 => '*',
        198..=219 => '#',
        220..=241 => '$',
        _ => '@',
    }
}

fn clear_screen() {
    // 使用移动光标到开始位置代替完全清屏
    print!("\x1B[H");
}
