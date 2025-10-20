#[cfg(not(feature = "async"))]
fn main () {}

#[cfg(feature = "async")]
fn main() {
    use std::{env, path::Path, process};

    use indicatif::ProgressBar;

    let progress = ProgressBar::no_length();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let args: Vec<String> = env::args().collect();
            if args.len() != 3 {
                eprintln!("Usage: {} <url> <destination>", args[0]);
                process::exit(1);
            }

            let url = &args[1];
            let destination = Path::new(&args[2]);
            let client = downlowd::Client::new();
            match client
                .get(url)
                .destination(destination)
                .on_progress(move |p| {
                    if let Some(total) = p.remote_length() {
                        progress.set_length(total);
                        progress.set_style(
                            indicatif::ProgressStyle::with_template(
                                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                            )
                            .unwrap()
                            .progress_chars("#>-"),
                        );
                    }
                    progress.set_position(p.bytes());
                })
                .send()
                .await
            {
                Ok(result) => {
                    println!(
                        "Downloaded {} bytes to {:?}",
                        result.bytes_downloaded, result.path
                    );
                }
                Err(e) => {
                    eprintln!("Error downloading file: {e}");
                    process::exit(1);
                }
            }
        });
}
