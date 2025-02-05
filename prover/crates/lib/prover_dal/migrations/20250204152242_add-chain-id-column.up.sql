ALTER TABLE witness_inputs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE leaf_aggregation_witness_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_aggregation_witness_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recursion_tip_witness_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scheduler_witness_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE proof_compression_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri_archive ADD COLUMN chain_id INTEGER NOT NULL DEFAULT 0;

ALTER TABLE witness_inputs_fri DROP CONSTRAINT IF EXISTS witness_inputs_fri_pkey;
ALTER TABLE recursion_tip_witness_jobs_fri DROP CONSTRAINT IF EXISTS recursion_tip_witness_jobs_fri_pkey;
ALTER TABLE scheduler_witness_jobs_fri DROP CONSTRAINT IF EXISTS scheduler_witness_jobs_fri_pkey;
ALTER TABLE proof_compression_jobs_fri DROP CONSTRAINT IF EXISTS proof_compression_jobs_fri_pkey;

ALTER TABLE witness_inputs_fri ADD CONSTRAINT witness_inputs_fri_pkey PRIMARY KEY (l1_batch_number, chain_id);
ALTER TABLE recursion_tip_witness_jobs_fri ADD CONSTRAINT recursion_tip_witness_jobs_fri_pkey PRIMARY KEY (l1_batch_number, chain_id);
ALTER TABLE scheduler_witness_jobs_fri ADD CONSTRAINT scheduler_witness_jobs_fri_pkey PRIMARY KEY (l1_batch_number, chain_id);
ALTER TABLE proof_compression_jobs_fri ADD CONSTRAINT proof_compression_jobs_fri_pkey PRIMARY KEY (l1_batch_number, chain_id);
