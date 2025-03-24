use colored::*;
use secp256k1::hashes::hex::DisplayHex;
use secp256k1::{All, Secp256k1};
use secp256k1::{Message, PublicKey, SecretKey};
use secp256k1::hashes::{sha256, Hash};
use hex::encode;
use reqwest::Client;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use serde::Deserialize;

pub struct Solution {
    pub public_key: PublicKey,
    pub private_key: SecretKey,
    pub server: String,
    pub hash: String,
    pub rewards_dir: String,
    pub on_mined: String,
    pub reward: f64,
}

#[derive(Deserialize)]
struct Response {
    id: u64
}

impl Solution {
    pub async fn submit(&self, secp: &Secp256k1<All>, total_mined: &mut f64) {
        let digest = sha256::Hash::hash(encode(self.public_key.serialize_uncompressed()).as_bytes());
        let sign = secp.sign_ecdsa(&Message::from_digest(digest.to_byte_array()), &self.private_key);
        let public_key_str = self.public_key.serialize_uncompressed().to_hex_string(secp256k1::hashes::hex::Case::Lower);

        println!("{} Signature: {}", "[INFO]".blue(), sign);
        println!("{} Public key: {}", "[INFO]".blue(), public_key_str);
        println!("{} Hash: {}", "[INFO]".blue(), self.hash);
        println!("{} {}", "[INFO]".blue(), "Submitting...".green());

        let url = format!(
            "{}/challenge-solved?holder={}&sign={}&hash={}",
            self.server, public_key_str, sign, self.hash
        );

        let client = Client::new();
        match client.get(&url).send().await {
            Ok(res) => {
                if res.status().is_success() {
                    println!("{} {}\n", "[INFO]".blue(), "Successfully submitted.".green());
                    *total_mined += self.reward;
                    match res.json::<Response>().await {
                        Ok(response) => {
                            if !Path::new(&self.rewards_dir).exists() {
                                let _ = fs::create_dir(&self.rewards_dir);
                            }
                            match fs::File::create(&format!("{}/{}.coin", &self.rewards_dir, response.id)) {
                                Ok(mut file) => {
                                    let _ = file.write_all(format!("{}", self.private_key.display_secret()).as_bytes());
                                    #[cfg(target_os = "windows")]
                                    {
                                        let output = Command::new("cmd")
                                            .args(&["/C", &self.on_mined.replace("%cid%", &response.id.to_string())])
                                            .output();
                                        println!("{} {}", "[CMD OUT]".blue(), String::from_utf8_lossy(&output.unwrap().stdout));
                                    }

                                    #[cfg(not(target_os = "windows"))]
                                    {
                                        let output = Command::new("sh")
                                            .arg("-c")
                                            .arg(&self.on_mined.replace("%cid%", &response.id.to_string()))
                                            .output();
                                        println!("{} {}", "[CMD OUT]".blue(), String::from_utf8_lossy(&output.unwrap().stdout));
                                    }
                                },
                                Err(e) => {
                                    println!("{} .coin file creation failed: {}\n", "[ERROR]".red(), e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("{} JSON deserialization failed: {}\n", "[ERROR]".red(), e);
                        }
                    }
                } else {
                    let text = res.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    println!("{} Failed to submit, message: {}", "[ERROR]".red(), text);
                }
            }
            Err(e) => {
                println!("{} Request failed: {}\n", "[ERROR]".red(), e);
            }
        }
    }
}
