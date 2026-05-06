use super::*;

impl App {
    pub fn cycle_tab_next(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Mission => ActiveTab::Release,
            ActiveTab::Release => ActiveTab::Jobs,
            ActiveTab::Jobs => ActiveTab::Agents,
            ActiveTab::Agents => ActiveTab::Tests,
            ActiveTab::Tests => ActiveTab::Pools,
            ActiveTab::Pools => ActiveTab::Cache,
            ActiveTab::Cache => ActiveTab::Evidence,
            ActiveTab::Evidence => ActiveTab::Secrets,
            ActiveTab::Secrets => ActiveTab::Git,
            ActiveTab::Git => ActiveTab::Mission,
        };
    }

    pub fn cycle_pane_next(&mut self) {
        // Only Jobs is currently rendered; cycling to Pools/Pipelines would silently
        // focus invisible panes. Expand this when those panes are visible.
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn cycle_pane_prev(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn up(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                if self.selected_test_index > 0 {
                    self.selected_test_index -= 1;
                } else {
                    self.selected_test_index = limit - 1;
                }
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    if self.selected_pool_index > 0 {
                        self.selected_pool_index -= 1;
                    } else {
                        self.selected_pool_index = self.state.pools.len() - 1;
                    }
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    } else {
                        self.selected_pipeline_index = self.state.pipelines.len() - 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    } else {
                        self.selected_job_index = self.state.recent_jobs.len() - 1;
                    }
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub fn down(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                self.selected_test_index = (self.selected_test_index + 1) % limit;
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    self.selected_pool_index =
                        (self.selected_pool_index + 1) % self.state.pools.len();
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    self.selected_pipeline_index =
                        (self.selected_pipeline_index + 1) % self.state.pipelines.len();
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    self.selected_job_index =
                        (self.selected_job_index + 1) % self.state.recent_jobs.len();
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub(crate) fn update_log_target(&mut self) {
        if self.maximize_logs
            && let Some(job) = self.selected_job()
        {
            let target = Some(LogTarget {
                project_id: job.project_id,
                job_id: job.job_id,
            });
            if self.log_target != target {
                self.log_target = target;
                let _ = self.log_target_tx.send(target);
            }
            return;
        }
        if self.log_target.is_some() {
            self.log_target = None;
            let _ = self.log_target_tx.send(None);
        }
    }

    pub(crate) fn sync_selected_job_index(&mut self) {
        if self.state.recent_jobs.is_empty() {
            self.selected_job_index = 0;
            self.selected_job_id = None;
            return;
        }

        if let Some(job_id) = self.selected_job_id
            && let Some(index) = self
                .state
                .recent_jobs
                .iter()
                .position(|job| job.job_id == job_id)
        {
            self.selected_job_index = index;
            return;
        }

        if self.selected_job_index >= self.state.recent_jobs.len() {
            self.selected_job_index = self.state.recent_jobs.len() - 1;
        }
        self.remember_selected_job();
    }

    pub(crate) fn remember_selected_job(&mut self) {
        self.selected_job_id = self.selected_job().map(|job| job.job_id);
    }

    pub fn selected_job(&self) -> Option<&JobEvent> {
        self.state.recent_jobs.get(self.selected_job_index)
    }

    pub fn open_selected_job_log(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.remember_selected_job();
        self.maximize_logs = true;
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
        self.update_log_target();
    }

    pub fn close_log_view(&mut self) {
        self.maximize_logs = false;
        self.update_log_target();
    }

    pub fn scroll_logs_up(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(amount);
    }

    pub fn follow_logs(&mut self) {
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
    }

    pub fn jump_logs_top(&mut self) {
        self.follow_log_tail = false;
        self.log_scroll_offset = 0;
    }

    pub async fn toggle_pool_paused(&mut self) -> Result<()> {
        if let Some(pool) = self.state.pools.get(self.selected_pool_index) {
            if pool.paused {
                crate::pool::resume_pool(&self.store, &self.gitlab, &pool.name).await?;
            } else {
                crate::pool::pause_pool(&self.store, &self.gitlab, &pool.name).await?;
            }
        }
        Ok(())
    }

    pub async fn remove_selected_item(&mut self) -> Result<()> {
        match self.active_pane {
            ActivePane::Pipelines => {
                if let Some(pm) = self.state.pipelines.get(self.selected_pipeline_index) {
                    let pid = pm.pipeline.pipeline_id;
                    self.store.delete_pipeline(pid).await?;
                    // Remove from local state immediately for snappy UX
                    self.state.pipelines.remove(self.selected_pipeline_index);
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
                    let jid = j.job_id;
                    self.store.delete_job_event(jid).await?;
                    self.state.recent_jobs.remove(self.selected_job_index);
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn requeue_selected_job(&mut self) -> Result<()> {
        if self.active_pane == ActivePane::Jobs
            && let Some(j) = self.state.recent_jobs.get(self.selected_job_index)
            && j.status == "failed"
        {
            self.gitlab.requeue_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub fn toggle_test_view_mode(&mut self) {
        self.test_view_mode = match self.test_view_mode {
            TestViewMode::Average => TestViewMode::Latest,
            TestViewMode::Latest => TestViewMode::Average,
        };
        self.selected_test_index = 0;
        self.selected_test_history = None;
    }

    pub async fn fetch_selected_test_history(&mut self) {
        let bottlenecks = match self.test_view_mode {
            TestViewMode::Average => &self.state.test_bottlenecks_avg,
            TestViewMode::Latest => &self.state.test_bottlenecks_latest,
        };
        if let Some(b) = bottlenecks.get(self.selected_test_index)
            && let Ok(hist) = self.store.get_test_history(&b.test_name, 50).await
        {
            self.selected_test_history = Some(hist);
        }
    }

    // -----------------------------------------------------------------------
    // TUI v2 — Runner feed controls
    // -----------------------------------------------------------------------

    pub fn feed_next(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            self.state.active_feed_index =
                (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_prev(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            if self.state.active_feed_index > 0 {
                self.state.active_feed_index -= 1;
            } else {
                self.state.active_feed_index = self.state.runner_feeds.len() - 1;
            }
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_toggle_pin(&mut self) {
        if self.feed_pinned.is_some() {
            self.feed_pinned = None;
        } else {
            self.feed_pinned = Some(self.state.active_feed_index);
        }
    }

    pub fn feed_follow_toggle(&mut self) {
        self.feed_follow_tail = !self.feed_follow_tail;
        if self.feed_follow_tail {
            self.feed_scroll_offset = u16::MAX;
        }
    }

    // TUI v2 — Interactive actions

    pub async fn cancel_selected_job(&mut self) -> Result<()> {
        if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
            self.gitlab.cancel_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub async fn force_refresh(&mut self) {
        self.refresh_now().await;
    }
}
