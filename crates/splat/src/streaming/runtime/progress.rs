//! User-facing stream activity; no renderer readback or optional diagnostics needed.
use super::*;

impl SogStream {
    /// Describe actual displayed levels separately from in-flight refinement.
    pub fn loading_task(&self) -> Option<ProgressTask> {
        let label = crate::assets::loading_label(&self.name);
        if let Some(error) = self.submission_error.as_ref().or(self.last_error.as_ref()) {
            return Some(
                ProgressTask::new(label)
                    .detail(error)
                    .state(ProgressTaskState::Error),
            );
        }
        let downloads = self
            .entries
            .iter()
            .filter(|entry| entry.loading.is_some())
            .count();
        let preparing = self.snapshot.is_pending() || self.needs_upload_frame();
        if !self.pending_work && !preparing && downloads == 0 {
            return None;
        }

        let displayed: HashSet<_> = self
            .displayed
            .iter()
            .map(|r| (r.file, r.offset, r.count))
            .collect();
        let mut shown = std::collections::BTreeSet::new();
        let mut loading = std::collections::BTreeSet::new();
        let mut regions = 0;
        for leaf in &self.manifest.leaves {
            let mut visible = false;
            for (level, range) in &leaf.lods {
                if range.count > 0 && displayed.contains(&(range.file, range.offset, range.count)) {
                    shown.insert(*level);
                    visible = true;
                }
                if self
                    .entries
                    .get(range.file)
                    .is_some_and(|e| e.loading.is_some())
                {
                    loading.insert(*level);
                }
            }
            regions += usize::from(visible);
        }
        let mut lines = Vec::new();
        if downloads > 0 {
            lines.push(format!(
                "Loading detail · {downloads} chunks{}",
                levels(&loading).map_or(String::new(), |lod| format!(" · {lod}"))
            ));
        }
        if preparing {
            lines.push(
                if self.displayed.is_empty() {
                    "Preparing first view"
                } else {
                    "Preparing updated detail"
                }
                .into(),
            );
        } else if downloads == 0 {
            lines.push(
                if self.displayed.is_empty() {
                    "Loading initial coverage"
                } else {
                    "Refining visible detail"
                }
                .into(),
            );
        }
        if let Some(lod) = levels(&shown) {
            let count = self
                .displayed
                .iter()
                .fold(0usize, |n, r| n.saturating_add(r.count));
            lines.push(format!(
                "Displayed: {lod} · {regions} regions · {:.2}M splats",
                count as f64 / 1_000_000.
            ));
        }
        Some(ProgressTask::new(label).detail(lines.join("\n")))
    }
}

fn levels(levels: &std::collections::BTreeSet<usize>) -> Option<String> {
    let first = levels.first()?;
    let last = levels.last()?;
    Some(if first == last {
        format!("LOD {first}")
    } else {
        format!("LOD {first}–{last}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream() -> SogStream {
        SogStream::parse(
            br#"{"lodLevels":6,"filenames":["fine/meta.json","coarse/meta.json"],
            "tree":{"bound":{"min":[0,0,0],"max":[1,1,1]},"lods":{
                "0":{"file":0,"offset":0,"count":100},
                "5":{"file":1,"offset":0,"count":10}}}}"#,
            "https://example.test/lod-meta.json",
            "lod-meta.json".into(),
            "test".into(),
        )
        .unwrap()
    }

    #[test]
    fn requested_fine_detail_is_not_reported_as_displayed() {
        let mut stream = stream();
        stream.displayed = vec![stream.manifest.leaves[0].lods[1].1];
        stream.active = vec![stream.manifest.leaves[0].lods[0].1];
        stream.submitted = stream.active.clone();
        stream.entries[0].loading = Some(ChunkLoad::pending(0.));
        let task = stream.loading_task().unwrap();
        assert!(!task.label.contains("json"));
        assert!(task.detail.contains("Loading detail · 1 chunks · LOD 0"));
        assert!(task.detail.contains("Displayed: LOD 5 · 1 regions"));

        stream.entries[0].loading = None;
        stream.preparing_upload = true;
        let task = stream.loading_task().unwrap();
        assert!(task.detail.contains("Preparing updated detail"));
        assert!(!task.detail.contains("Loading detail"));
        assert!(task.detail.contains("Displayed: LOD 5"));
        stream.preparing_upload = false;
        assert!(stream.loading_task().is_none());
    }

    #[test]
    fn errors_do_not_look_like_an_endless_refinement() {
        let mut stream = stream();
        stream.pending_work = true;
        stream.submission_error = Some("Unable to prepare scene".into());
        assert_eq!(
            stream.loading_task().unwrap().state,
            ProgressTaskState::Error
        );
    }
}
