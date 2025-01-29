use axum::Json;
use zksync_dal::CoreDal;
use zksync_prover_interface::api::{SubmitProofRequest, SubmitProofResponse};
use zksync_types::{
    commitment::serialize_commitments, web3::keccak256, L1BatchNumber, ProtocolVersionId, H256,
    STATE_DIFF_HASH_KEY_PRE_GATEWAY,
};

use crate::{api::RequestProcessor, errors::RequestProcessorError};

impl RequestProcessor {
    pub(crate) async fn submit_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        payload: SubmitProofRequest,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        tracing::info!("Received proof for block number: {:?}", l1_batch_number);
        match payload {
            SubmitProofRequest::Proof(proof) => {
                let blob_url = self
                    .blob_store
                    .put((l1_batch_number, proof.protocol_version()), &*proof)
                    .await
                    .map_err(RequestProcessorError::ObjectStore)?;

                let aggregation_coords = proof.aggregation_result_coords();

                let system_logs_hash_from_prover = H256::from_slice(&aggregation_coords[0]);
                let state_diff_hash_from_prover = H256::from_slice(&aggregation_coords[1]);
                let bootloader_heap_initial_content_from_prover =
                    H256::from_slice(&aggregation_coords[2]);
                let events_queue_state_from_prover = H256::from_slice(&aggregation_coords[3]);

                let mut storage = self.pool.connection().await.unwrap();

                let l1_batch = storage
                    .blocks_dal()
                    .get_l1_batch_metadata(l1_batch_number)
                    .await
                    .unwrap()
                    .expect("Proved block without metadata");

                let protocol_version = l1_batch
                    .header
                    .protocol_version
                    .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

                let events_queue_state = l1_batch
                    .metadata
                    .events_queue_commitment
                    .expect("No events_queue_commitment");
                let bootloader_heap_initial_content = l1_batch
                    .metadata
                    .bootloader_initial_content_commitment
                    .expect("No bootloader_initial_content_commitment");

                if events_queue_state != events_queue_state_from_prover
                    || bootloader_heap_initial_content
                        != bootloader_heap_initial_content_from_prover
                {
                    panic!(
                        "Auxilary output doesn't match\n\
                        server values: events_queue_state = {events_queue_state}, bootloader_heap_initial_content = {bootloader_heap_initial_content}\n\
                        prover values: events_queue_state = {events_queue_state_from_prover}, bootloader_heap_initial_content = {bootloader_heap_initial_content_from_prover}",
                    );
                }

                let system_logs = serialize_commitments(&l1_batch.header.system_logs);
                let system_logs_hash = H256(keccak256(&system_logs));

                let state_diff_hash = if protocol_version.is_pre_gateway() {
                    l1_batch
                        .header
                        .system_logs
                        .iter()
                        .find_map(|log| {
                            (log.0.key
                                == H256::from_low_u64_be(STATE_DIFF_HASH_KEY_PRE_GATEWAY as u64))
                            .then_some(log.0.value)
                        })
                        .expect("Failed to get state_diff_hash from system logs")
                } else {
                    l1_batch
                        .metadata
                        .state_diff_hash
                        .expect("Failed to get state_diff_hash from metadata")
                };

                if state_diff_hash != state_diff_hash_from_prover
                    || system_logs_hash != system_logs_hash_from_prover
                {
                    let server_values = format!("system_logs_hash = {system_logs_hash}, state_diff_hash = {state_diff_hash}");
                    let prover_values = format!("system_logs_hash = {system_logs_hash_from_prover}, state_diff_hash = {state_diff_hash_from_prover}");
                    panic!(
                        "Auxilary output doesn't match, server values: {} prover values: {}",
                        server_values, prover_values
                    );
                }

                storage
                    .proof_generation_dal()
                    .save_proof_artifacts_metadata(l1_batch_number, &blob_url)
                    .await
                    .map_err(RequestProcessorError::Dal)?;
            }
            SubmitProofRequest::SkippedProofGeneration => {
                self.pool
                    .connection()
                    .await
                    .unwrap()
                    .proof_generation_dal()
                    .mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await
                    .map_err(RequestProcessorError::Dal)?;
            }
        }

        Ok(Json(SubmitProofResponse::Success))
    }
}
