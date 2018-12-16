extern crate env_logger;
extern crate getopts;
extern crate image;
extern crate m3d;
extern crate tiff;
extern crate vangers;


use std::io::BufWriter;
use std::fs::File;
use std::path::PathBuf;

fn import_image(path: &PathBuf) -> vangers::level::LevelData {
    println!("\tLoading the image...");
    let image = image::open(path).unwrap().to_rgba();
    println!("\tImporting the level...");
    let size = (image.width() as i32, image.height() as i32);
    vangers::level::LevelData::import(&image.into_raw(), size)
}

pub fn save_tiff(path: &PathBuf, layers: vangers::level::LevelLayers) {
    let images = [
        tiff::Image {
            width: layers.size.0 as u32,
            height: layers.size.1 as u32,
            bpp: 8,
            name: "h0",
            data: &layers.het0,
        },
        tiff::Image {
            width: layers.size.0 as u32,
            height: layers.size.1 as u32,
            bpp: 8,
            name: "h1",
            data: &layers.het1,
        },
        tiff::Image {
            width: layers.size.0 as u32,
            height: layers.size.1 as u32,
            bpp: 8,
            name: "del",
            data: &layers.delta,
        },
        tiff::Image {
            width: layers.size.0 as u32,
            height: layers.size.1 as u32,
            bpp: 4,
            name: "m0",
            data: &layers.mat0,
        },
        tiff::Image {
            width: layers.size.0 as u32,
            height: layers.size.1 as u32,
            bpp: 4,
            name: "m1",
            data: &layers.mat1,
        },
    ];

    let file = BufWriter::new(File::create(path).unwrap());
    tiff::save(file, &images).unwrap();
}


fn main() {
    use std::env;
    env_logger::init().unwrap();

    let args: Vec<_> = env::args().collect();
    let mut options = getopts::Options::new();
    options
        .parsing_style(getopts::ParsingStyle::StopAtFirstFree)
        .optflag("h", "help", "print this help menu");

    let matches = options.parse(&args[1 ..]).unwrap();
    if matches.opt_present("h") || matches.free.len() != 2 {
        println!("Vangers resource converter");
        let brief = format!(
            "Usage: {} [options] <input> <output>",
            args[0]
        );
        println!("{}", options.usage(&brief));
        return;
    }

    let src_path = PathBuf::from(matches.free[0].as_str());
    let dst_path = PathBuf::from(matches.free[1].as_str());

    match (
        src_path.extension().and_then(|ostr| ostr.to_str()).unwrap_or(""),
        dst_path.extension().and_then(|ostr| ostr.to_str()).unwrap_or(""),
    ) {
        ("m3d", "ron") => {
            let file = File::open(&src_path).unwrap();
            println!("\tLoading M3D...");
            let raw = m3d::FullModel::load(file);
            println!("\tExporting OBJ data...");
            raw.export_obj(&dst_path);
        }
        ("ron", "md3") => {
            println!("\tImporting OBJ data...");
            let model = m3d::FullModel::import_obj(&src_path);
            println!("\tSaving M3D...");
            model.save(File::create(&dst_path).unwrap());
        }
        ("ini", "bmp") | ("ini", "png") | ("ini", "tga") => {
            println!("\tLoading the level...");
            let config = vangers::level::LevelConfig::load(&src_path);
            let level = vangers::level::load(&config);
            let data = level.export();
            println!("\tSaving the image...");
            image::save_buffer(
                &dst_path, &data,
                level.size.0 as u32, level.size.1 as u32,
                image::ColorType::RGBA(8),
            ).unwrap();
        }
        ("ini", "tiff") => {
            println!("\tLoading the level...");
            let config = vangers::level::LevelConfig::load(&src_path);
            let layers = vangers::level::load(&config).export_layers();
            println!("\tSaving TIFF layers...");
            save_tiff(&dst_path, layers);
        }
        ("ini", "vmp") => {
            println!("\tLoading the VMC...");
            let config = vangers::level::LevelConfig::load(&src_path);
            let level = vangers::level::load(&config);
            println!("\tSaving VMP...");
            vangers::level::LevelData::from(level).save_vmp(&dst_path);
        }
        ("bmp", "vmc") | ("png", "vmc") | ("tga", "vmc") => {
            let level = import_image(&src_path);
            println!("\tSaving VMC...");
            level.save_vmc(&dst_path);
        }
        ("bmp", "vmp") | ("png", "vmp") | ("tga", "vmp") => {
            let level = import_image(&src_path);
            println!("\tSaving VMP...");
            level.save_vmp(&dst_path);
        }
        (in_ext, out_ext) => {
            panic!("Don't know how to convert {} to {}", in_ext, out_ext);
        }
    }
}
