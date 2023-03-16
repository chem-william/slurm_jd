use chrono::prelude::*;
use clap::Parser;
use colored::Colorize;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use std::str;

const INPUT_DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const LOG_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const START_END_FORMAT: &str = "%b-%d %H:%M";
const FORMAT_CMD: [&str; 7] = [
    "jobid",
    "jobname%30",
    "alloccpus",
    "elapsed",
    "start",
    "end",
    "state",
];
const N_CMDS: usize = FORMAT_CMD.len();
const WIDTH: usize = 24;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Get finished jobs from the last 24h
    #[clap(long)]
    day: bool,
}

#[derive(Debug)]
struct Job {
    jobid: usize,
    jobname: String,
    alloccpus: usize,
    elapsed: String,
    start: NaiveDateTime,
    end: NaiveDateTime,
    state: String,
}
impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.jobid == other.jobid
    }
}
impl Eq for Job {}

impl Job {
    fn parse_job(lines: &[&str], date_format: &str) -> Self {
        Job {
            jobid: lines[0].parse::<usize>().expect("could not parse jobid"),
            jobname: lines[1].to_string(),
            alloccpus: lines[2]
                .parse::<usize>()
                .expect("could not parse alloccpus"),
            elapsed: lines[3].to_string(),
            start: match lines[4] {
                // placeholder value as the job is not yet started
                "Unknown" => NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
                    NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap(),
                ),
                _ => NaiveDateTime::parse_from_str(lines[4], date_format)
                    .expect("unable to parse start"),
            },
            end: match lines[5] {
                // placeholder value due to the job being unfinished
                "Unknown" => NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
                    NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap(),
                ),
                _ => NaiveDateTime::parse_from_str(lines[5], date_format)
                    .expect("unable to parse end"),
            },
            state: lines[6].to_string(),
        }
    }
}

fn convert_to_string(input_bytes: Vec<u8>) -> String {
    match String::from_utf8(input_bytes) {
        Ok(v) => v,
        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
    }
}

fn call_sacct(format_cmd: [&str; 7], last_session: &str) -> String {
    let output = Command::new("sacct")
        .args(["-u", "williamb", "-n", "-S", last_session])
        .arg(format!("--format={}", format_cmd.join(",")))
        .output()
        .expect("failed to execute process");

    if output.status.success() {
        convert_to_string(output.stdout)
    } else {
        convert_to_string(output.stderr)
    }
}

fn get_finished_jobs(sacct_output: String) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let split_output: Vec<_> = sacct_output.split_whitespace().collect();

    for (idx, line) in split_output.iter().enumerate() {
        if line.parse::<f64>().is_ok() && line.len() > 3 {
            let mut tmp_job: [&str; N_CMDS] = [""; N_CMDS];
            tmp_job[..N_CMDS].copy_from_slice(&split_output[idx..(N_CMDS + idx)]);

            // some jobs have .x - skip those
            if tmp_job[0].parse::<usize>().is_ok() {
                let job = Job::parse_job(&tmp_job, INPUT_DATE_FORMAT);

                // Don't bother with jobs that are still running
                if job.state != "RUNNING" {
                    jobs.push(job)
                }
            }
        }
    }

    // skip the first job as it's erroneously reported by SLURM
    // jobs.into_iter().skip(1).collect()
    jobs
}

fn create_print(jobs: &Vec<Job>) -> Vec<String> {
    let mut job_messages: Vec<_> = Vec::with_capacity(32);
    let skip_states = ["PENDING", "Unkown", "CANCELLED+"];
    for job in jobs {
        if !skip_states.iter().any(|&x| job.state == x) {
            let jobid = job.jobid;
            let jobname = &job.jobname;
            let alloccpus = job.alloccpus;
            let elapsed = &job.elapsed;
            let start = job.start.format(START_END_FORMAT);
            let end = job.end.format(START_END_FORMAT);
            let state = if job.state == "COMPLETED" {
                job.state.green()
            } else {
                job.state.red()
            };
            let message = format!(
                "{jobid:<9} {jobname:jobname_width$} {alloccpus:<6} {elapsed:<13} {start:<13} {end:<14} {state}", jobname_width = WIDTH - 1
            );

            job_messages.push(message);
        }
    }

    job_messages
}

fn log_jobs(jobs: Vec<Job>, log_file: PathBuf) {
    let mut fd = File::create(&log_file).expect("unable to open log_file");
    if log_file.exists() {
        for job in jobs {
            writeln!(
                fd,
                "{};{};{};{};{};{};{}",
                job.jobid, job.jobname, job.alloccpus, job.elapsed, job.start, job.end, job.state
            )
            .expect("unable to write to log_file");
        }
    }
}

fn save_date(date_file: PathBuf) {
    let mut fd = File::create(&date_file).expect("unable to open log_file");
    if date_file.exists() {
        write!(fd, "{}", Local::now().naive_local().format(LOG_DATE_FORMAT))
            .expect("unable to write date to date_file");
    }
}

fn get_last_session(date_file: &PathBuf) -> NaiveDateTime {
    if date_file.exists() {
        let contents = fs::read_to_string(date_file).expect("unable to read date file");
        if contents.is_empty() {
            let mut file = File::create(date_file).expect("Unable to create new prev_job");
            let now = Local::now().naive_local();
            write!(file, "{}", now.format(LOG_DATE_FORMAT))
                .expect("unable to write date to empty date_file");
            now
        } else {
            NaiveDateTime::parse_from_str(&contents, LOG_DATE_FORMAT)
                .expect("unable to parse date from date_file")
        }
    } else {
        let now = Local::now().naive_local();
        NaiveDateTime::new(now.date(), NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap())
    }
}

fn main() {
    let args = Args::parse();

    let mut log_file = std::env::current_exe().unwrap();
    log_file.pop();
    log_file.push("log_file");

    let mut date_file = std::env::current_exe().unwrap();
    date_file.pop();
    date_file.push("date_file");

    let last_session = get_last_session(&date_file);
    let formatted_last_session = last_session.format(START_END_FORMAT).to_string().yellow();

    let sacct_output = if args.day {
        call_sacct(FORMAT_CMD, "00:00")
    } else {
        call_sacct(
            FORMAT_CMD,
            &last_session.format(INPUT_DATE_FORMAT).to_string(),
        )
    };
    let jobs = get_finished_jobs(sacct_output);

    let job_messages = create_print(&jobs);

    if !job_messages.is_empty() {
        if args.day {
            println!("{}", "Jobs completed today:".bold().underline());
        } else {
            println!(
                "{} {}",
                "Jobs completed since last session:".bold().underline(),
                formatted_last_session
            );
        }

        let mut headers = String::with_capacity(32);
        for header in FORMAT_CMD {
            let tmp = match header {
                "alloccpus" => "CPUs   ".bold().to_string(),
                "jobid" => "Job ID    ".bold().to_string(),
                "elapsed" => "Elapsed       ".bold().to_string(),
                "start" => "Start         ".bold().to_string(),
                "end" => "End            ".bold().to_string(),
                "state" => "State    ".bold().to_string(),
                "jobname%30" => format!("{:WIDTH$}", "Job Name".bold()),
                _ => panic!("more header states than expected"),
            };
            headers.push_str(&tmp);
        }
        println!("{}", headers);

        for job in job_messages {
            println!("{}", job);
        }
    } else {
        println!(
            "{} {}",
            "No jobs have finished since".bold().underline(),
            formatted_last_session
        );
    }

    log_jobs(jobs, log_file);
    save_date(date_file);
}
