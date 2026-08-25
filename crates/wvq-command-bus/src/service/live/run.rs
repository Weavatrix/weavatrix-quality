//! Inherent LiveService run orchestrator.

use super::super::access::*;
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        self.run_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }

    pub(in crate::service) fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        let prepared = self.prepare_controlled_run(cmd)?;
        let executed = self.execute_controlled_run(cmd, cancel, &prepared)?;
        self.persist_controlled_run(cmd, &prepared, executed)
    }
}
