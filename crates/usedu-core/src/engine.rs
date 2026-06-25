use crate::scanner::{
    scan_current_level, scan_recursive, CurrentLevelScan, FileSummary, ScanOptions, ScanResult,
};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub root: PathBuf,
    pub options: ScanOptions,
}

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub result: ScanResult,
}

#[derive(Debug, Clone)]
pub struct CurrentLevelOutcome {
    pub result: CurrentLevelScan,
}

#[derive(Debug, Clone, Default)]
pub struct ScanEngine;

pub trait Collector {
    type Output;

    fn collect(&self, outcome: &ScanOutcome) -> Self::Output;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub used_bytes: u64,
    pub regular_file_count: u64,
    pub directory_count: u64,
    pub symlink_count: u64,
    pub other_count: u64,
    pub issue_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SummaryCollector;

#[derive(Debug, Clone)]
pub struct TopKCollector {
    pub limit: usize,
}

impl ScanEngine {
    pub fn scan(&self, request: ScanRequest) -> Result<ScanOutcome> {
        Ok(ScanOutcome {
            result: scan_recursive(request.root, &request.options)?,
        })
    }

    pub fn scan_current_level(&self, request: ScanRequest) -> Result<CurrentLevelOutcome> {
        Ok(CurrentLevelOutcome {
            result: scan_current_level(request.root, &request.options)?,
        })
    }
}

impl Collector for SummaryCollector {
    type Output = Summary;

    fn collect(&self, outcome: &ScanOutcome) -> Self::Output {
        let root = &outcome.result.root;
        Summary {
            used_bytes: root.used_bytes,
            regular_file_count: root.counts.regular_files,
            directory_count: root.counts.directories,
            symlink_count: root.counts.symlinks,
            other_count: root.counts.other,
            issue_count: root.errors.len() as u64,
        }
    }
}

impl Collector for TopKCollector {
    type Output = Vec<FileSummary>;

    fn collect(&self, outcome: &ScanOutcome) -> Self::Output {
        outcome
            .result
            .top_files
            .iter()
            .take(self.limit)
            .cloned()
            .collect()
    }
}
