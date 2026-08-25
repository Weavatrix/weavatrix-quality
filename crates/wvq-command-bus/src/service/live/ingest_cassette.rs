//! Admit a HAR archive as a privacy-safe network cassette. Never enables replay.

use super::super::access::*;
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn ingest_cassette(
        &self,
        cmd: &IngestCassetteCommand,
    ) -> Result<IngestCassetteReply, BusError> {
        let admitted = ingest_har(&cmd.har, &cmd.origin)
            .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        let revision = self.revision()?.to_string();
        if admitted.captured_entries == 0 {
            return Ok(cassette_reply(
                cmd.origin.clone(),
                revision,
                admitted,
                None,
            ));
        }
        let body = serde_json::to_vec(&admitted.profile)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let digest = sha256_hex(&body);
        let handle = format!("artifact-cassette-{}", &digest[..16.min(digest.len())]);
        let artifact =
            ArtifactId::new(&handle).map_err(|err| BusError::Identity(err.to_string()))?;
        self.store()?
            .put_artifact(&artifact, NETWORK_CASSETTE_KIND, &body)
            .map_err(|err| BusError::Store(err.to_string()))?;
        Ok(cassette_reply(
            cmd.origin.clone(),
            revision,
            admitted,
            Some(handle),
        ))
    }
}

fn cassette_reply(
    origin: String,
    revision: String,
    admitted: wvq_runtime::CassetteAdmission,
    profile_handle: Option<String>,
) -> IngestCassetteReply {
    let useful = admitted.captured_entries != 0;
    IngestCassetteReply {
        origin,
        revision,
        captured_entries: admitted.captured_entries,
        omitted: admitted.omitted,
        useful,
        discarded: !useful,
        discard_reason: (!useful).then(|| "no_json_same_origin_responses".into()),
        limitations: admitted.limitations,
        replay_enabled: false,
        seal_eligible: false,
        profile_handle,
        runtime_llm_tokens: 0,
    }
}
