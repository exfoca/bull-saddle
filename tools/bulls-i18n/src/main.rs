// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use bulls_i18n::{add_locale, coverage, missing, validate};

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bulls-i18n: {message}");
            ExitCode::from(1)
        }
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut arguments = arguments.into_iter();
    let first = arguments.next().ok_or_else(usage)?;
    let (root, command) = if first == "--root" {
        let root = arguments
            .next()
            .ok_or_else(|| "--root requires a path".to_owned())?;
        let command = arguments.next().ok_or_else(usage)?;
        (PathBuf::from(root), command)
    } else {
        (PathBuf::from("crates/bulls-cli/i18n"), first)
    };

    match command.as_str() {
        "check" => {
            ensure_no_arguments(arguments)?;
            let report = validate(&root).map_err(|error| error.to_string())?;
            if !report.is_valid() {
                for issue in report.issues() {
                    eprintln!("{}: {}", issue.code(), issue.message());
                }
                return Err(format!(
                    "validation failed with {} issue(s)",
                    report.issues().len()
                ));
            }
            println!("i18n check: OK");
        }
        "coverage" => {
            ensure_no_arguments(arguments)?;
            print_coverage(coverage(&root).map_err(|error| error.to_string())?);
        }
        "missing" => {
            let locale = arguments
                .next()
                .ok_or_else(|| usage_for("missing <locale>"))?;
            ensure_no_arguments(arguments)?;
            let messages = missing(&root, &locale).map_err(|error| error.to_string())?;
            if messages.is_empty() {
                println!("{locale}: complete");
            } else {
                for message in messages {
                    println!("{}\t{}", message.message(), message.reason());
                }
            }
        }
        "add-locale" => {
            let locale = arguments
                .next()
                .ok_or_else(|| usage_for("add-locale <locale>"))?;
            ensure_no_arguments(arguments)?;
            let path = add_locale(&root, &locale).map_err(|error| error.to_string())?;
            println!("created {}", path.display());
            println!(
                "translations are pending; `bulls-i18n check` must fail until coverage is complete"
            );
        }
        _ => return Err(usage()),
    }

    Ok(())
}

fn print_coverage(report: bulls_i18n::CoverageReport) {
    let message_width = report
        .rows()
        .iter()
        .map(|row| row.message().len())
        .max()
        .unwrap_or("Message".len())
        .max("Message".len());
    let locale_widths = report
        .locales()
        .iter()
        .map(|locale| locale.len().max(3))
        .collect::<Vec<_>>();

    print!("{:<message_width$}", "Message");
    for (locale, width) in report.locales().iter().zip(&locale_widths) {
        print!("  {:>width$}", locale, width = *width);
    }
    println!();

    for row in report.rows() {
        print!("{:<message_width$}", row.message());
        for (status, width) in row.locale_status().iter().zip(&locale_widths) {
            print!(
                "  {:>width$}",
                if *status { "yes" } else { "no" },
                width = *width
            );
        }
        println!();
    }

    println!();
    let totals = report.locale_totals();
    for ((locale, total), count) in report
        .locales()
        .iter()
        .zip(totals)
        .zip(std::iter::repeat(report.rows().len()))
    {
        let percentage = if count == 0 {
            100.0
        } else {
            total as f64 * 100.0 / count as f64
        };
        println!("{locale:<10} {total}/{count} {percentage:>6.1}%");
    }
}

fn ensure_no_arguments(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if arguments.next().is_some() {
        Err(usage())
    } else {
        Ok(())
    }
}

fn usage_for(command: &str) -> String {
    format!("usage: bulls-i18n [--root <path>] {command}")
}

fn usage() -> String {
    "usage: bulls-i18n [--root <path>] <check|coverage|missing <locale>|add-locale <locale>>"
        .to_owned()
}
