//! Scaffold `.weavatrix-quality/` without inventing tests, browser, or a model.

use super::super::access::*;
use super::LiveService;

/// Fail-closed starter policy. Bindings, browser, and AI stay absent until real.
pub(in crate::service) const DEFAULT_QUALITY_POLICY: &str = "\
quality_policy_v: 1

ratchet:
  mode: no_new_debt
  returned_debt: error

ui_integrity:
  enabled: true
";

const QUALITY_GITIGNORE: &str = "\
# Evidence ledger, CAS objects, and generated runner reports.
# Keep config.yaml in version control.
quality.db
quality.db-wal
quality.db-shm
objects/
runtime/
junit.xml
go-cover.out
";

const CONFIG_REL: &str = ".weavatrix-quality/config.yaml";
const GITIGNORE_REL: &str = ".weavatrix-quality/.gitignore";

impl LiveService {
    pub(in crate::service) fn init(&self, cmd: &InitCommand) -> Result<InitReply, BusError> {
        if !self.repo.is_dir() {
            return Err(BusError::InvalidInput(format!(
                "init requires a directory, got {}",
                self.repo.display()
            )));
        }
        let quality = self.repo.join(".weavatrix-quality");
        let config = quality.join("config.yaml");
        if config.is_file() && !cmd.force {
            return Err(BusError::InvalidInput(
                "quality policy already exists; pass --force true to replace it".into(),
            ));
        }
        std::fs::create_dir_all(&quality).map_err(|err| {
            BusError::Runtime(format!("cannot create {}: {err}", quality.display()))
        })?;

        let mut created = Vec::new();
        let mut skipped = Vec::new();
        write_file(&config, DEFAULT_QUALITY_POLICY)?;
        created.push(CONFIG_REL.into());

        let gitignore = quality.join(".gitignore");
        if gitignore.is_file() && !cmd.force {
            skipped.push(GITIGNORE_REL.into());
        } else {
            write_file(&gitignore, QUALITY_GITIGNORE)?;
            created.push(GITIGNORE_REL.into());
        }

        Ok(InitReply {
            created,
            skipped,
            config: CONFIG_REL.into(),
            runtime_llm_tokens: 0,
        })
    }
}

fn write_file(path: &Path, contents: &str) -> Result<(), BusError> {
    std::fs::write(path, contents)
        .map_err(|err| BusError::Runtime(format!("cannot write {}: {err}", path.display())))
}
