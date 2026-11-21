//! What follows the first download pass of a usenet job: PAR2 verify, fetching only the recovery
//! volumes the damage needs, repair, then the enabled post-processors in order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fangst_postproc_par2::{self as par2, Set, Verification, Volume};
use fangst_providers::{JobContext, PostProcessOutcome};

use crate::progress::Meter;
use crate::{Error, Job, Runner};

/// NZB indices of recovery volumes that wait until verification asks for them. A set without an
/// index file keeps its volumes in the first pass, since one of them has to describe the set.
pub(crate) fn held_volumes(names: &[String], sets: &[Set]) -> HashSet<usize> {
    let by_name = index_by_name(names);
    sets.iter()
        .filter(|set| set.index.is_some())
        .flat_map(|set| &set.volumes)
        .filter_map(|volume| by_name.get(volume.name.as_str()).copied())
        .collect()
}

fn index_by_name(names: &[String]) -> HashMap<&str, usize> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect()
}

/// Runs PAR2 work on a blocking thread; a panic inside the PAR2 code fails the job, not the runner.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, par2::Error> + Send + 'static,
) -> Result<T, par2::Error> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|e| Err(par2::Error::Repair(format!("the PAR2 step stopped: {e}"))))
}

/// The state of one job while its PAR2 sets are checked.
struct Pass<'a> {
    job_id: i64,
    nzb: &'a fangst_nzb::Nzb,
    out: &'a Path,
    by_name: HashMap<&'a str, usize>,
    /// NZB indices fetched so far in this run.
    downloaded: HashSet<usize>,
    meter: &'a Arc<Meter>,
}

impl Runner {
    /// Verifies every PAR2 set and repairs the damaged ones. Returns why the job fails, if it does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn verify_and_repair(
        &self,
        job_id: i64,
        nzb: &fangst_nzb::Nzb,
        out: &Path,
        names: &[String],
        sets: &[Set],
        report: &fangst_usenet::Report,
        meter: &Arc<Meter>,
    ) -> Result<Option<String>, Error> {
        let mut pass = Pass {
            job_id,
            nzb,
            out,
            by_name: index_by_name(names),
            downloaded: report.files.iter().map(|f| f.index).collect(),
            meter,
        };
        let mut covered: HashSet<String> = HashSet::new();
        for set in sets {
            // A volume repeats the index's packets, so it describes the set as well.
            let Some(descriptor) = set.index.clone().or_else(|| {
                set.volumes
                    .iter()
                    .min_by_key(|v| v.blocks)
                    .map(|v| v.name.clone())
            }) else {
                continue;
            };
            let path = out.join(&descriptor);
            if !path.exists() {
                continue;
            }
            let folder = out.to_owned();
            let verification = match blocking(move || par2::verify(&folder, &path)).await {
                Ok(verification) => verification,
                Err(e) => {
                    // The files are then judged by what arrived, as without PAR2.
                    self.store
                        .add_event(job_id, "verify", &format!("{descriptor}: {e}"))
                        .await?;
                    continue;
                }
            };
            self.store
                .add_event(job_id, "verify", &format!("{descriptor}: {verification}"))
                .await?;
            if !verification.is_intact()
                && let Some(reason) = self
                    .repair_set(&mut pass, set, &descriptor, &verification)
                    .await?
            {
                return Ok(Some(reason));
            }
            covered.extend(verification.files);
        }

        // Files no PAR2 set vouches for must have arrived whole. A damaged PAR2 file only matters
        // through the verify and repair above.
        let incomplete: Vec<&fangst_usenet::FileReport> = report
            .files
            .iter()
            .filter(|f| !f.is_complete() && !par2::is_par2(&f.name) && !covered.contains(&f.name))
            .collect();
        if incomplete.is_empty() {
            return Ok(None);
        }
        let missing: usize = incomplete.iter().map(|f| f.missing_segments.len()).sum();
        let failed: usize = incomplete.iter().map(|f| f.failed_segments.len()).sum();
        Ok(Some(format!(
            "{missing} segments missing on the server, {failed} could not be downloaded"
        )))
    }

    /// Fetches recovery volumes until the set repairs or none are left. Returns why it failed.
    async fn repair_set(
        &self,
        pass: &mut Pass<'_>,
        set: &Set,
        descriptor: &str,
        verification: &Verification,
    ) -> Result<Option<String>, Error> {
        let needed = verification.blocks_needed;
        let on_disk = set
            .volumes
            .iter()
            .filter(|v| {
                pass.by_name
                    .get(v.name.as_str())
                    .is_some_and(|i| pass.downloaded.contains(i))
            })
            .fold(0u32, |sum, v| sum.saturating_add(v.blocks));
        // Recovery blocks still to fetch, judged by volume names until a repair attempt counts them.
        let mut short = needed.saturating_sub(on_disk);
        loop {
            if short > 0 {
                let candidates: Vec<(usize, Volume)> = set
                    .volumes
                    .iter()
                    .filter_map(|v| pass.by_name.get(v.name.as_str()).map(|&i| (i, v.clone())))
                    .filter(|(i, _)| !pass.downloaded.contains(i))
                    .collect();
                let volumes: Vec<Volume> = candidates.iter().map(|(_, v)| v.clone()).collect();
                let Some(picked) = par2::choose_volumes(&volumes, short) else {
                    let available = volumes.iter().fold(needed.saturating_sub(short), |sum, v| {
                        sum.saturating_add(v.blocks)
                    });
                    return Ok(Some(
                        par2::Error::NotEnoughRecovery { needed, available }.to_string(),
                    ));
                };
                let selected: HashSet<usize> = picked.iter().map(|&p| candidates[p].0).collect();
                let blocks: u32 = picked.iter().map(|&p| volumes[p].blocks).sum();
                self.store
                    .add_event(
                        pass.job_id,
                        "repair",
                        &format!(
                            "fetching {} recovery volumes with {blocks} blocks for {descriptor}",
                            picked.len()
                        ),
                    )
                    .await?;
                if let Err(e) = self
                    .fetch(pass.job_id, pass.nzb, pass.out, &selected, pass.meter)
                    .await?
                {
                    return Ok(Some(e.to_string()));
                }
                pass.downloaded.extend(selected);
            }

            let folder = pass.out.to_owned();
            let path: PathBuf = pass.out.join(descriptor);
            match blocking(move || par2::repair(&folder, &path)).await {
                Ok(repaired) => {
                    self.store
                        .add_event(
                            pass.job_id,
                            "repair",
                            &format!(
                                "{descriptor}: repaired {} blocks in {} files",
                                repaired.blocks, repaired.files
                            ),
                        )
                        .await?;
                    return Ok(None);
                }
                // Volumes can arrive damaged themselves and then hold fewer blocks than named.
                Err(par2::Error::NotEnoughRecovery { needed, available }) if available < needed => {
                    short = needed - available;
                }
                Err(e) => return Ok(Some(e.to_string())),
            }
        }
    }

    /// Runs the post-processors in order. Returns why the job fails, if one fails it.
    pub(crate) async fn post_process(
        &self,
        job: &Job,
        nzb: &fangst_nzb::Nzb,
        out: &Path,
    ) -> Result<Option<String>, Error> {
        for processor in &self.post_processors {
            let id = processor.info().id;
            let context = JobContext {
                job_id: job.id,
                name: job.name.clone(),
                category: job.category.clone(),
                folder: out.to_owned(),
                password: nzb.password().map(str::to_owned),
            };
            match processor.process(context).await {
                Ok(PostProcessOutcome::Continue { message }) => {
                    self.store
                        .add_event(job.id, "postprocess", &format!("{id}: {message}"))
                        .await?;
                }
                Ok(PostProcessOutcome::Fail { reason }) => {
                    return Ok(Some(format!("{id}: {reason}")));
                }
                Err(e) => return Ok(Some(format!("{id}: {e}"))),
            }
        }
        Ok(None)
    }
}
