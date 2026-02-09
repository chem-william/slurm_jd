use anyhow::{Context, Result};
use chrono::prelude::*;
use clap::Parser;
use colored::Colorize;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;
use std::process::Command;
use std::str;

const INPUT_DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const LOG_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const START_END_FORMAT: &str = "%b-%d %H:%M";
const FORMAT_CMD: [&str; 7] = [
    "jobid%20",
    "jobname%30",
    "alloccpus",
    "elapsed",
    "start",
    "end",
    "state",
];
const N_CMDS: usize = FORMAT_CMD.len();
const WIDTH: usize = 24;
const SKIP_STATES: [&str; 2] = ["PENDING", "CANCELLED+"];

#[derive(Debug, PartialEq)]
enum ParsedJobId {
    Singular(usize),
    Array { base: usize, index: usize },
    NotJob,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Get finished jobs from the last 24 hours
    #[clap(long, conflicts_with_all = ["since", "hours", "days"])]
    day: bool,

    /// Get finished jobs since a specific time (YYYY-MM-DDTHH:MM:SS)
    #[clap(long, value_name = "YYYY-MM-DDTHH:MM:SS", conflicts_with_all = ["hours", "days"])]
    since: Option<String>,

    /// Get finished jobs from othe last N hours
    hours: Option<i64>,

    /// Get finished jobs from the last N days
    days: Option<i64>,

    /// SLURM username
    #[clap(short, long, default_value_t = default_user())]
    user: String,
}

fn default_user() -> String {
    std::env::var("USER").expect("expected default user to be available")
}

#[derive(Debug)]
struct Job {
    jobid_base: usize,
    array_index: Option<usize>,
    jobname: String,
    alloccpus: usize,
    elapsed: String,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    state: String,
}
impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.jobid_base == other.jobid_base && self.array_index == other.array_index
    }
}
impl Eq for Job {}

impl Job {
    fn parse_job(
        jobid_base: usize,
        array_index: Option<usize>,
        lines: &[&str],
        date_format: &str,
    ) -> Self {
        Job {
            jobid_base,
            array_index,
            jobname: lines[1].to_string(),
            alloccpus: lines[2]
                .parse::<usize>()
                .expect("could not parse alloccpus"),
            elapsed: lines[3].to_string(),
            start: match lines[4] {
                // placeholder value as the job is not yet (UNKNOWN)/was never (NONE) started
                "Unknown" | "None" => None,
                _ => Some(
                    NaiveDateTime::parse_from_str(lines[4], date_format)
                        .expect("unable to parse start"),
                ),
            },
            end: match lines[5] {
                // placeholder value due to the job being unfinished
                "Unknown" => None,
                _ => Some(
                    NaiveDateTime::parse_from_str(lines[5], date_format)
                        .expect("unable to parse end"),
                ),
            },
            state: lines[6].to_string(),
        }
    }

    fn is_displayable(&self) -> bool {
        !SKIP_STATES.iter().any(|&x| self.state == x)
    }

    fn jobid_display(&self) -> String {
        if let Some(idx) = self.array_index {
            format!("{}_{}", self.jobid_base, idx)
        } else {
            self.jobid_base.to_string()
        }
    }
}

fn call_sacct(format_cmd: [&str; 7], window_start: NaiveDateTime, user: &str) -> String {
    let output = Command::new("sacct")
        .args([
            "-u",
            user,
            "-n",
            "-S",
            &window_start.format(INPUT_DATE_FORMAT).to_string(),
        ])
        .arg(format!("--format={}", format_cmd.join(",")))
        .output()
        .expect("failed to execute process");

    let bytes = if output.status.success() {
        output.stdout
    } else {
        output.stderr
    };
    String::from_utf8(bytes).unwrap_or_else(|e| panic!("Invalid UTF-8 sequence: {e}"))
}

fn check_job(line: &str) -> ParsedJobId {
    // if the jobid contains '.' it's a sub-job (batch, extern, etc.)
    if line.contains('.') {
        return ParsedJobId::NotJob;
    }

    if let Some((job_base, array_index)) = line.split_once('_') {
        if let (Ok(base), Ok(index)) = (job_base.parse::<usize>(), array_index.parse::<usize>()) {
            return ParsedJobId::Array { base, index };
        }
        return ParsedJobId::NotJob;
    }
    if let Ok(id) = line.parse::<usize>() {
        return ParsedJobId::Singular(id);
    }

    ParsedJobId::NotJob
}

fn get_finished_jobs(sacct_output: &str) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let split_output: Vec<_> = sacct_output.split_whitespace().collect();

    for chunked_lines in split_output.chunks(N_CMDS) {
        let parsed_jobid = check_job(chunked_lines[0]);
        let (base_id, array_index) = match parsed_jobid {
            ParsedJobId::Singular(id) => (id, None),
            ParsedJobId::Array { base, index } => (base, Some(index)),
            ParsedJobId::NotJob => continue,
        };
        let job = Job::parse_job(base_id, array_index, chunked_lines, INPUT_DATE_FORMAT);
        if job.state != "RUNNING" {
            jobs.push(job);
        }
    }

    jobs
}

fn format_job_line(jobid: &str, job: &Job, indent: &str) -> String {
    let jobname = &job.jobname;
    let alloccpus = job.alloccpus;
    let elapsed = &job.elapsed;
    let start = if let Some(job_start) = job.start {
        job_start.format(START_END_FORMAT).to_string().white()
    } else {
        "NOT STARTED".yellow()
    };
    let end = if let Some(job_end) = job.end {
        job_end.format(START_END_FORMAT).to_string().white()
    } else {
        "UNKNOWN".yellow()
    };
    let state = if job.state == "COMPLETED" {
        job.state.green()
    } else {
        job.state.red()
    };
    let id_width = 15 - indent.len();
    format!(
        "{indent}{jobid:<id_width$} {jobname:jobname_width$} {alloccpus:<6} {elapsed:<13} {start:<13} {end:<14} {state}",
        jobname_width = WIDTH - 1
    )
}

fn create_print(jobs: &[Job]) -> Vec<String> {
    let mut job_messages: Vec<_> = Vec::with_capacity(32);
    let mut i = 0;
    while i < jobs.len() {
        let job = &jobs[i];
        if !job.is_displayable() {
            i += 1;
            continue;
        }

        if job.array_index.is_some() {
            let base = job.jobid_base;
            let jobname = &job.jobname;

            // Collect printable array elements
            let start = i;
            let mut has_printable = false;
            while i < jobs.len() && jobs[i].jobid_base == base && jobs[i].array_index.is_some() {
                if jobs[i].is_displayable() {
                    has_printable = true;
                }
                i += 1;
            }

            if has_printable {
                // Parent header: jobid_base and jobname, no state
                let header = format!(
                    "{:<15} {jobname:jobname_width$}",
                    base,
                    jobname_width = WIDTH - 1
                );
                job_messages.push(header);

                // Indented child lines
                for child in &jobs[start..i] {
                    if child.is_displayable() {
                        let child_id = format!("{}", child.array_index.unwrap());
                        job_messages.push(format_job_line(&child_id, child, "  "));
                    }
                }
            }
        } else {
            let jobid = job.jobid_display();
            job_messages.push(format_job_line(&jobid, job, ""));
            i += 1;
        }
    }

    job_messages
}

fn log_jobs(jobs: &[Job], log_file: &Path) -> Result<()> {
    let mut fd = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .context("Failed to open log file")?;
    for job in jobs {
        writeln!(
            fd,
            "{};{};{};{};{:?};{:?};{}",
            job.jobid_display(),
            job.jobname,
            job.alloccpus,
            job.elapsed,
            job.start,
            job.end,
            job.state
        )?;
    }
    Ok(())
}

fn save_date(date_file: &Path) {
    let mut fd = File::create(date_file).expect("unable to open date_file");
    write!(fd, "{}", Local::now().naive_local().format(LOG_DATE_FORMAT))
        .expect("unable to write date to date_file");
}

fn get_last_session(date_file: &Path) -> NaiveDateTime {
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
        NaiveDateTime::new(
            now.date(),
            NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap(),
        )
    }
}

fn main() {
    let args = Args::parse();

    let mut log_file = std::env::current_exe().expect("could not acquire log file");
    log_file.pop();
    log_file.push("log_file");

    let mut date_file = std::env::current_exe().expect("could not acquire date file");
    date_file.pop();
    date_file.push("date_file");

    let last_session = get_last_session(&date_file);
    let now = Local::now().naive_local();
    let window_start = if let Some(since) = args.since.as_deref() {
        NaiveDateTime::parse_from_str(since, INPUT_DATE_FORMAT)
            .expect("unable to parse --since with expected format")
    } else if let Some(hours) = args.hours {
        now - chrono::Duration::hours(hours)
    } else if let Some(days) = args.days {
        now - chrono::Duration::days(days)
    } else if args.day {
        now - chrono::Duration::days(1)
    } else {
        last_session
    };
    let formatted_window_start = window_start.format(START_END_FORMAT).to_string().yellow();

    let sacct_output = call_sacct(FORMAT_CMD, window_start, &args.user);
    let jobs = get_finished_jobs(&sacct_output);

    let job_messages = create_print(&jobs);

    if job_messages.is_empty() {
        println!(
            "{} {}",
            "No jobs have finished since".bold().underline(),
            formatted_window_start
        );
    } else {
        println!(
            "{} {}",
            "Jobs completed since:".bold().underline(),
            formatted_window_start
        );
        let mut headers = String::with_capacity(32);
        for header in FORMAT_CMD {
            let tmp = match header {
                "alloccpus" => "CPUs   ".bold().to_string(),
                "jobid%20" => "Job ID          ".bold().to_string(),
                "elapsed" => "Elapsed       ".bold().to_string(),
                "start" => "Start         ".bold().to_string(),
                "end" => "End            ".bold().to_string(),
                "state" => "State    ".bold().to_string(),
                "jobname%30" => format!("{:WIDTH$}", "Job Name".bold()),
                _ => panic!("got unexpected header state: {header}"),
            };
            headers.push_str(&tmp);
        }
        println!("{headers}");

        for job in job_messages {
            println!("{job}");
        }
    }

    log_jobs(&jobs, &log_file).unwrap();
    save_date(&date_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! jobtypes_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (input, expected) = $value;
                assert_eq!(expected, check_job(input));
            }
    )*
        }
    }
    jobtypes_tests! {
        check_job0: ("39122024_15+", ParsedJobId::NotJob),
        check_job1: ("39122024_3.1", ParsedJobId::NotJob),
        check_job2: ("39122024.1", ParsedJobId::NotJob),
        check_job3: ("39122024.1+", ParsedJobId::NotJob),
        check_job4: ("39122024_16+", ParsedJobId::NotJob),
        check_job5: ("39122024_16", ParsedJobId::Array { base: 39122024, index: 16 }),
        check_job6: ("39122024", ParsedJobId::Singular(39122024)),
        check_job7: ("56938944_10.batch", ParsedJobId::NotJob),
        check_job8: ("56938944_10.extern", ParsedJobId::NotJob),
        check_job9: ("56938944_10", ParsedJobId::Array { base: 56938944, index: 10 }),
        check_job10: ("56938942.batch", ParsedJobId::NotJob),
    }

    macro_rules! parse_job_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (jobid_base, array_index, input, expected) = $value;
                let job = Job::parse_job(jobid_base, array_index, &input, INPUT_DATE_FORMAT);
                assert_eq!(expected.jobid_base, job.jobid_base);
                assert_eq!(expected.array_index, job.array_index);
                assert_eq!(expected.jobname, job.jobname);
                assert_eq!(expected.alloccpus, job.alloccpus);
                assert_eq!(expected.elapsed, job.elapsed);
                assert_eq!(expected.start, job.start);
                assert_eq!(expected.end, job.end);
                assert_eq!(expected.state, job.state);
            }
    )*
        }
    }
    parse_job_tests! {
        parse_job0: (
            39139726_usize, None,
            ["39139726", "1e-2", "84", "00:08:58", "2023-04-22T16:15:05", "2023-04-22T16:24:03", "COMPLETED"],
            Job{
                jobid_base: 39139726,
                array_index: None,
                jobname: "1e-2".to_string(),
                alloccpus: 84,
                elapsed: "00:08:58".to_string(),
                start: Some(NaiveDateTime::parse_from_str("2023-04-22T16:15:05", INPUT_DATE_FORMAT).unwrap()),
                end: Some(NaiveDateTime::parse_from_str("2023-04-22T16:24:03", INPUT_DATE_FORMAT).unwrap()),
                state: "COMPLETED".to_string()
            }
        ),
        parse_job1: (
            50280159_usize, None,
            ["50280159", "MultiprocessDistances", "4", "20:27:32", "2025-03-19T19:32:54", "Unknown", "FAILED"],
            Job{
                jobid_base: 50280159,
                array_index: None,
                jobname: "MultiprocessDistances".to_string(),
                alloccpus: 4,
                elapsed: "20:27:32".to_string(),
                start: Some(NaiveDateTime::parse_from_str("2025-03-19T19:32:54", INPUT_DATE_FORMAT).unwrap()),
                end: None,
                state: "FAILED".to_string()
            }
        ),
        parse_job_array: (
            56938944_usize, Some(3_usize),
            ["56938944_3", "2JobArray", "2", "00:01:00", "2023-04-22T16:15:05", "2023-04-22T16:16:05", "COMPLETED"],
            Job{
                jobid_base: 56938944,
                array_index: Some(3),
                jobname: "2JobArray".to_string(),
                alloccpus: 2,
                elapsed: "00:01:00".to_string(),
                start: Some(NaiveDateTime::parse_from_str("2023-04-22T16:15:05", INPUT_DATE_FORMAT).unwrap()),
                end: Some(NaiveDateTime::parse_from_str("2023-04-22T16:16:05", INPUT_DATE_FORMAT).unwrap()),
                state: "COMPLETED".to_string()
            }
        ),
    }

    #[test]
    fn get_finished_jobs_with_arrays() {
        // Simulates the new sacct output format (jobid, jobname, alloccpus, elapsed, start, end, state)
        let sacct_output = "\
            56938942 SingularJob 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938942.batch batch 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938942.extern extern 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_1 ArrayJob 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_1.batch batch 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_1.extern extern 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_2 ArrayJob 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_2.batch batch 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED \
            56938944_2.extern extern 2 00:01:00 2023-04-22T16:15:05 2023-04-22T16:16:05 COMPLETED";

        let jobs = get_finished_jobs(sacct_output);
        assert_eq!(jobs.len(), 3);

        // First job: singular
        assert_eq!(jobs[0].jobid_base, 56938942);
        assert_eq!(jobs[0].array_index, None);
        assert_eq!(jobs[0].jobid_display(), "56938942");

        // Second job: array index 1
        assert_eq!(jobs[1].jobid_base, 56938944);
        assert_eq!(jobs[1].array_index, Some(1));
        assert_eq!(jobs[1].jobid_display(), "56938944_1");

        // Third job: array index 2
        assert_eq!(jobs[2].jobid_base, 56938944);
        assert_eq!(jobs[2].array_index, Some(2));
        assert_eq!(jobs[2].jobid_display(), "56938944_2");
    }

    #[test]
    fn jobid_display_formatting() {
        let singular = Job {
            jobid_base: 12345678,
            array_index: None,
            jobname: "test".to_string(),
            alloccpus: 1,
            elapsed: "00:00:01".to_string(),
            start: None,
            end: None,
            state: "COMPLETED".to_string(),
        };
        assert_eq!(singular.jobid_display(), "12345678");

        let array = Job {
            jobid_base: 12345678,
            array_index: Some(10),
            jobname: "test".to_string(),
            alloccpus: 1,
            elapsed: "00:00:01".to_string(),
            start: None,
            end: None,
            state: "COMPLETED".to_string(),
        };
        assert_eq!(array.jobid_display(), "12345678_10");
    }
}
