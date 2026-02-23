use colored::Colorize;
use serde_json::Value;

use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process;
use std::collections::HashMap;

fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        println!("gode-check v{}\nUsage: gode-check <release link> [artifact commit]", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    let url = &args[1];
    let parts = url.split('/').collect::<Vec<&str>>();
    if parts.len() < 8 {
        eprintln!("Invalid URL: {url}");
        process::exit(1);
    }

    let env_vars = env::vars().collect::<HashMap<String, String>>();
    let github_auth = env_vars.get("GODE_CHECK_GITHUB_TOKEN").map(|token| format!("Bearer {token}")).unwrap_or_default();

    let repo = format!("{}/{}", parts[3], parts[4]);
    let api_url = format!("https://api.github.com/repos/{repo}");
    let tag = parts[7];

    let client = reqwest::blocking::Client::new();

    let release = client
        .get(&format!("{api_url}/releases/tags/{tag}"))
        .header("Accept", "application/json")
        .header("User-Agent", "gode-check")
        .header("Authorization", github_auth.clone())
        .send()
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error fetching release: {:?}", e).red());
            process::exit(1);
        })
        .json::<Value>()
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error parsing release JSON: {:?}", e).red());
            process::exit(1);
        });

    let release_commit: String;
    if args.len() > 2 {
        release_commit = args[2].clone();

        println!("Using provided commit: {}", release_commit.cyan());
    } else {
        let release_object = client
            .get(&format!("{api_url}/git/refs/tags/{tag}"))
            .header("Accept", "application/json")
            .header("User-Agent", "gode-check")
            .header("Authorization", github_auth.clone())
            .send()
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error fetching tags: {:?}", e).red());
                process::exit(1);
            })
            .json::<Value>()
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error parsing tags JSON: {:?}", e).red());
                process::exit(1);
            });
        if release_object["object"]["type"].as_str().unwrap_or_default() != "commit" {
            let tag_sha = release_object["object"]["sha"].as_str().unwrap_or_default();
            let tag_object = client
                .get(&format!("{api_url}/git/tags/{tag_sha}"))
                .header("Accept", "application/json")
                .header("User-Agent", "gode-check")
                .header("Authorization", github_auth.clone())
                .send()
                .unwrap_or_else(|e| {
                    eprintln!("{}", format!("Error fetching tag object: {:?}", e).red());
                    process::exit(1);
                })
                .json::<Value>()
                .unwrap_or_else(|e| {
                    eprintln!("{}", format!("Error parsing tag object JSON: {:?}", e).red());
                    process::exit(1);
                });
            release_commit = tag_object["object"]["sha"].as_str().unwrap_or_default().to_string();
        } else {
            release_commit = release_object["object"]["sha"].as_str().unwrap_or_default().to_string();
        }

        println!("Release found for commit: {}", release_commit.cyan());
    }

    let artifacts = client
        .get(&format!("{api_url}/actions/artifacts"))
        .header("Accept", "application/json")
        .header("User-Agent", "gode-check")
        .header("Authorization", github_auth.clone())
        .send()
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error fetching artifacts: {:?}", e).red());
            process::exit(1);
        })
        .json::<Value>()
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error parsing artifacts JSON: {:?}", e).red());
            process::exit(1);
        })["artifacts"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|a| {
            a["workflow_run"]["head_sha"].as_str().unwrap_or_default().starts_with(&release_commit)
        })
        .cloned()
        .collect::<Vec<Value>>();

    let artifacts_len = artifacts.len();
    if artifacts_len < 1 {
        eprintln!("{}", "No artifacts found for the commit".red());
        process::exit(1);
    } else if artifacts_len > 1 {
        println!("{}", format!("{} artifacts for commit found!", artifacts_len).green());
    } else {
        println!("{}", "Artifact for commit found!".green());
    }

    let temp_dir = std::env::temp_dir().join("gode-check");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).unwrap_or_else(|e| {
            eprintln!("{}", format!("Failed to clean temporary directory: {:?}", e).red());
            process::exit(1);
        });
    }

    fs::create_dir(&temp_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Failed to create temporary directory: {:?}", e).red());
        process::exit(1);
    });

    let artifact_dir = temp_dir.join("artifact");
    fs::create_dir(&artifact_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Failed to create artifact directory: {:?}", e).red());
        process::exit(1);
    });

    let release_dir = temp_dir.join("release");
    fs::create_dir(&release_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Failed to create release directory: {:?}", e).red());
        process::exit(1);
    });

    let mut geode_files: Vec<Vec<PathBuf>> = vec![];

    for i in 0..artifacts_len {
        let artifact = &artifacts[i];
        let id = artifact["id"].as_u64().unwrap_or_default();
        let workflow_run_id = artifact["workflow_run"]["id"].as_u64().unwrap_or_default();

        if artifacts_len == 1 {
            println!("Downloading artifact...");
        } else {
            println!("Downloading artifact {}...", i + 1);
        }

        let artifact_path = artifact_dir.join(i.to_string());
        if !artifact_path.exists() {
            fs::create_dir(&artifact_path).unwrap_or_else(|e| {
                eprintln!("{}", format!("Failed to create artifact directory: {:?}", e).red());
                process::exit(1);
            });
        }

        let suite_id = client
            .get(&format!("{api_url}/actions/runs/{workflow_run_id}"))
            .header("Accept", "application/json")
            .header("User-Agent", "gode-check")
            .header("Authorization", github_auth.clone())
            .send()
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error fetching workflow run: {:?}", e).red());
                process::exit(1);
            })
            .json::<Value>()
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error parsing workflow run JSON: {:?}", e).red());
                process::exit(1);
            })["check_suite"]["id"].as_u64().unwrap_or_default();

        let mut zip_data: Cursor<Vec<u8>> = Cursor::new(vec![]);

        client
            .get(&format!("https://nightly.link/{repo}/suites/{suite_id}/artifacts/{id}"))
            .header("Accept", "application/octet-stream")
            .send()
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error downloading artifact: {:?}", e).red());
                process::exit(1);
            })
            .copy_to(&mut zip_data)
            .unwrap_or_else(|e| {
                eprintln!("{}", format!("Error reading artifact file: {:?}", e).red());
                process::exit(1);
            });

        let mut zip = zip::ZipArchive::new(zip_data).unwrap_or_else(|e| {
            eprintln!("{}", format!("Error reading zip archive: {:?}", e).red());
            process::exit(1);
        });
        zip.extract(&artifact_path).unwrap_or_else(|e| {
            eprintln!("{}", format!("Error extracting zip archive: {:?}", e).red());
            process::exit(1);
        });

        geode_files.push(fs::read_dir(&artifact_path).unwrap_or_else(|e| {
            eprintln!("{}", format!("Error reading artifact directory: {:?}", e).red());
            process::exit(1);
        }).filter_map(Result::ok).filter(|f| f.path().extension().map(|e| e == "geode").unwrap_or(false)).map(|f| f.path()).collect());
    }

    println!("Downloading release file...");

    let release_asset = release["assets"].as_array().unwrap_or(&vec![]).iter().find(|asset| {
        asset["name"].as_str().unwrap_or_default().ends_with(".geode")
    }).unwrap_or_else(|| {
        eprintln!("{}", "No .geode file found in the release".red());
        process::exit(1);
    }).clone();

    let release_path = release_dir.join(release_asset["name"].as_str().unwrap_or_default());
    client
        .get(release_asset["browser_download_url"].as_str().unwrap_or_default())
        .header("Accept", "application/octet-stream")
        .send()
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error downloading release file: {:?}", e).red());
            process::exit(1);
        })
        .copy_to(&mut fs::File::create(&release_path).unwrap_or_else(|e| {
            eprintln!("{}", format!("Error creating release file: {:?}", e).red());
            process::exit(1);
        }))
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error saving release file: {:?}", e).red());
            process::exit(1);
        });

    for i in 0..artifacts_len {
        if geode_files.len() > 1 {
            println!("{}", format!("Artifact {} ({}):", i + 1, &artifacts[i]["name"].as_str().unwrap_or_default()).yellow());
        }
        let artifact_files = &geode_files[i];
        for artifact_file in artifact_files {
            let artifact_data = fs::read(artifact_file).unwrap_or_else(|e| {
                eprintln!("{}", format!("Error opening artifact file: {:?}", e).red());
                process::exit(1);
            });
            let release_data = fs::read(&release_path).unwrap_or_else(|e| {
                eprintln!("{}", format!("Error opening release file: {:?}", e).red());
                process::exit(1);
            });

            let artifact_hash = sha256::digest(artifact_data);
            let release_hash = sha256::digest(release_data);

            if artifact_hash == release_hash {
                println!("{}", format!("{} {}", if artifact_files.len() > 1 {
                    format!("{} Comparison:", artifact_file.file_name().unwrap_or_default().to_string_lossy()).blue()
                } else {
                    "Comparison:".blue()
                }, "✅ Match".green()));
            } else {
                println!("{}", format!("{} {}", if artifact_files.len() > 1 {
                    format!("{} Comparison:", artifact_file.file_name().unwrap_or_default().to_string_lossy()).blue()
                } else {
                    "Comparison:".blue()
                }, "❌ Mismatch".red()));
            }

            println!("Artifact hash: {}", artifact_hash.cyan());
            println!("Release hash: {}", release_hash.cyan());
        }
    }
}
