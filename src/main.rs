use actix_files::NamedFile;
use actix_web::web;
use get_if_addrs::get_if_addrs;
use std::env::temp_dir;
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Read, Stdin};
use std::net::Ipv4Addr;

use clap::Parser;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tabled::{Table, Tabled};

struct DownloadFile {
    reader: Reader,
}

enum Reader {
    File(PathBuf),
    Stdin(StdinReader),
}

struct StdinReader {
    reader: Stdin,
    file: File,
}

async fn index(path: web::Data<PathBuf>) -> actix_web::Result<NamedFile> {
    Ok(NamedFile::open(path.as_ref())?)
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "-")]
    path: InputPath,
    #[arg(short, long, default_value = "9000")]
    port: u16,
}

#[derive(Clone, Debug)]
enum InputPath {
    Stdin,
    Path(PathBuf),
}

impl FromStr for InputPath {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "-" => Self::Stdin,
            path => Self::Path(Path::new(path).to_path_buf()),
        })
    }
}

#[derive(Tabled)]
struct ListeningOn {
    interface: String,
    address: Ipv4Addr,
    port: u16,
}

fn show_addresses(port: u16) -> Vec<ListeningOn> {
    match get_if_addrs() {
        Ok(interfaces) => interfaces
            .into_iter()
            .filter_map(|i_face| {
                if let get_if_addrs::IfAddr::V4(v4_addr) = i_face.addr.clone() {
                    if !v4_addr.is_loopback() {
                        Some(ListeningOn {
                            interface: i_face.name,
                            address: v4_addr.ip,
                            port,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to get interfaces: {}", e);
            vec![]
        }
    }
}

static mut FILE_CREATED: AtomicBool = AtomicBool::new(false);

struct InformerReader<R> {
    reader: R,
}

impl<R> Drop for InformerReader<R> {
    fn drop(&mut self) {
        unsafe { FILE_CREATED.fetch_or(true, Ordering::Relaxed) };
    }
}

impl<R: Read> Read for InformerReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    use actix_web::{web, App, HttpServer};

    let args = Args::parse();
    let port = args.port;
    let path = match args.path {
        InputPath::Stdin => {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_string();
            let path = temp_dir().join(timestamp);
            let file = File::create(&path)?;
            let mut writer = BufWriter::new(file);
            let mut reader = BufReader::new(io::stdin());
            io::copy(&mut reader, &mut writer)?;
            path
        }
        InputPath::Path(path) => path,
    };

    let data = web::Data::new(path);

    println!("Listening on:");
    println!("{}", Table::new(show_addresses(port)));
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .route("/", web::get().to(index))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
