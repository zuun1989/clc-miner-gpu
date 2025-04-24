use config::{Config, File};
use serde::Deserialize;
use colored::*;

#[derive(Debug, Deserialize)]
struct CLCMinerConfigLoad {
    pub server: String,
    pub submit_server: Option<String>,
    pub rewards_dir: String,
    pub thread: i64,
    #[serde(default)]
    pub on_mined: Option<String>,
    pub job_interval: Option<i64>,
    pub report_interval: Option<i64>,
    pub pool_secret: Option<String>,
    pub reporting: Option<Reporting>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Reporting {
    pub report_server: String,
    pub report_user: String,
}

pub struct CLCMinerConfig {
    pub server: String,
    pub rewards_dir: String,
    pub thread: i64,
    pub job_interval: i64,
    pub report_interval: i64,
    pub on_mined: String,
    pub reporting: Reporting,
    pub pool_secret: String,
    pub submit_server: String
}

pub fn load() -> Result<CLCMinerConfig, String> {
    // Build the config and handle potential errors gracefully
    let settings = Config::builder()
        .add_source(File::with_name("clcminer.toml"))
        .build();

    match settings {
        Ok(s) => {
            // Deserialize the config to CLCMinerConfig
            match s.try_deserialize::<CLCMinerConfigLoad>() {
                Ok(config) => {
                    let reporting: Reporting = match &config.reporting {
                        Some(reporting) => reporting.clone(),  // Clone the Reporting struct
                        None => Reporting {
                            report_server: String::from(""),
                            report_user: String::from(""),
                        },
                    };
                    let submit_server: String = match &config.submit_server {
                        Some(submit_server) => submit_server.to_string(),  // Convert &String to String
                        None => String::from("https://master.centrix.fi"),
                    };
                    let on_mined: String = match &config.on_mined {
                        Some(on_mined) => on_mined.to_string(),  // Convert &String to String
                        None => String::from(""),
                    };
                    let job_interval: i64 = match &config.job_interval {
                        Some(job_interval) => *job_interval,
                        None => 1,
                    };
                    let report_interval: i64 = match &config.report_interval {
                        Some(report_interval) => *report_interval,
                        None => 1,
                    };
                    let pool_secret: String = match &config.pool_secret {
                        Some(pool_secret) => pool_secret.to_string(),
                        None => String::from(""),
                    };

                    return Ok(CLCMinerConfig {
                        server: config.server,
                        submit_server: submit_server,
                        rewards_dir: config.rewards_dir,
                        thread: config.thread,
                        job_interval: job_interval,
                        report_interval: report_interval,
                        on_mined: on_mined,
                        reporting: reporting,
                        pool_secret: pool_secret
                    });
                },
                Err(e) => {
                    eprintln!("{} {:?}", "[ERROR] Failed to deserialize config:".red(), e);
                    Err("Failed to deserialize config".into())
                }
            }
        }
        Err(e) => {
            eprintln!("{} {:?}", "[ERROR] Failed to load clcminer.toml:".red(), e);
            Err("Failed to deserialize config".into())
        }
    }
}