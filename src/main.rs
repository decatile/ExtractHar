use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::exit,
    str::FromStr,
};

use base64::Engine;
use clap::Parser;
use serde::Deserialize;
use url::Url;

macro_rules! pexit {
    ($($arg:tt)*) => {{
        println!($($arg)*);
        exit(1);
    }};
}

#[derive(Deserialize)]
struct Har {
    log: HarLog,
}

#[derive(Deserialize)]
struct HarLog {
    entries: Vec<HarLogEntry>,
}

#[derive(Deserialize)]
struct HarLogEntry {
    request: HarLogEntryRequest,
    response: HarLogEntryResponse,
}

#[derive(Deserialize)]
struct HarLogEntryRequest {
    url: Url,
}

#[derive(Deserialize)]
struct HarLogEntryResponse {
    content: HarLogEntryResponseContent,
}

#[derive(Deserialize)]
struct HarLogEntryResponseContent {
    #[serde(default)]
    text: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
}

#[derive(Parser)]
struct Cli {
    input_har: String,
    output_dir: Option<String>,
    #[arg(long, default_value = None)]
    output_domain: Option<String>,
    #[arg(long, default_value = None)]
    output_path: Option<String>,
    #[arg(long, default_value_t = 0)]
    output_path_depth: i32,
}

fn get_mimetypes() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("image/webp", ".webp");
    map.insert("image/jpeg", ".jpeg");
    map.insert("image/jpeg", ".jpg");
    map.insert("image/png", ".png");
    map.insert("image/svg+xml", ".svg");
    return map;
}

fn main() {
    let Cli {
        input_har,
        output_dir,
        output_domain,
        output_path,
        output_path_depth,
    } = Cli::parse();
    let input_file_path = Path::new(&input_har)
        .canonicalize()
        .unwrap_or_else(|_| pexit!("Cannot parse path {}", input_har));
    if !input_file_path.is_file() {
        pexit!("Specified path ({}) is not a file", input_har);
    }
    let folder = if let Some(arg) = output_dir {
        PathBuf::from_str(&arg).unwrap_or_else(|_| {
            pexit!("Cannot parse path {}", arg);
        })
    } else {
        input_file_path.with_file_name({
            let mut without_ext = input_file_path
                .with_extension("")
                .file_name()
                .unwrap()
                .to_owned();
            without_ext.push("_extract");
            without_ext
        })
    };
    if !folder.is_dir() {
        fs::create_dir_all(&folder).unwrap_or_else(|_| {
            pexit!("Cannot create dirs at path {}", folder.to_string_lossy());
        });
    }
    println!("Loading file");
    let input_file = File::open(&input_file_path).unwrap_or_else(|_| pexit!("Cannot open file"));
    let har = serde_json::from_reader::<_, Har>(input_file).unwrap_or_else(|err| {
        pexit!("Cannot parse file as json to .har model: {:?}", err);
    });
    println!("Extraction output settings:");
    if output_domain.is_none() && output_path.is_none() {
        println!(
            "- do not create any directory structure - extract images directly to base folder"
        );
    } else {
        if output_domain.is_none() {
            pexit!("--output_domain is required in this context");
        }
        println!(
            "- create subfolders for domain {}",
            output_domain.as_ref().unwrap()
        );
        if let Some(path) = &output_path {
            println!(
                " - create subfolders for URL path: {} (only for {} {} parts)",
                path,
                if output_path_depth > 0 {
                    "first"
                } else {
                    "last"
                },
                output_path_depth.abs()
            )
        }
    }
    println!("Starting extraction...");
    let mime_types = get_mimetypes();
    let mime_type_extensions = mime_types.values().collect::<Vec<_>>();
    let mut count_total = 0;
    let mut count_extracted = 0;
    for entry in har.log.entries {
        count_total += 1;
        let mime_type = entry.response.content.mime_type;
        if let Some(ext) = mime_types.get(mime_type.as_str()) {
            count_extracted += 1;
            let url = entry.request.url;
            let url_host = url.host_str().unwrap();
            let url_segments = url.path_segments().unwrap().collect::<Vec<_>>();
            let url_path = &url_segments[..url_segments.len() - 1];
            let mut url_filename = url_segments[url_segments.len() - 1].to_string();
            if !mime_type_extensions
                .iter()
                .any(|x| url_filename.ends_with(x as &str))
            {
                url_filename.push_str(ext);
            }
            let path = if output_domain.is_some() && output_path.is_some() {
                let mut result = PathBuf::from_str(url_host).unwrap();
                url_path
                    .into_iter()
                    .for_each(|x| result.extend(Path::new(x)));
                Some(result)
            } else if output_domain.is_some() {
                Some(PathBuf::from_str(url_host).unwrap())
            } else if output_path.is_some() {
                let mut result = PathBuf::new();
                url_path
                    .into_iter()
                    .for_each(|x| result.extend(Path::new(x)));
                Some(result)
            } else {
                None
            };
            let sub_folder = if let Some(path) = &path {
                folder.join(path)
            } else {
                folder.clone()
            };
            let out_file = sub_folder.join(Path::new(&url_filename));
            if !sub_folder.is_dir() {
                fs::create_dir_all(sub_folder).unwrap();
            }
            let b64 = entry.response.content.text;
            let b = Engine::decode(&base64::engine::general_purpose::STANDARD, b64).unwrap();
            println!(
                "- {url_filename}: extracted to {} [{} bytes]",
                path.unwrap_or_else(|| folder.clone()).to_string_lossy(),
                b.len()
            );
            File::create(out_file).unwrap().write_all(&b).unwrap();
        }
    }
    println!("Finished extracting {count_extracted} (out of total {count_total}) files.")
}
