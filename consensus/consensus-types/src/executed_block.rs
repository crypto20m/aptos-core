// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    block::Block,
    common::{Payload, Round},
    quorum_cert::QuorumCert,
    vote_proposal::{MaybeSignedVoteProposal, VoteProposal},
};
use aptos_crypto::hash::HashValue;
use aptos_types::{
    block_info::BlockInfo,
    contract_event::ContractEvent,
    transaction::{Transaction, TransactionStatus},
};
use executor_types::StateComputeResult;
use std::fmt::{Debug, Display, Formatter};

/// ExecutedBlocks are managed in a speculative tree, the committed blocks form a chain. Besides
/// block data, each executed block also has other derived meta data which could be regenerated from
/// blocks.
#[derive(Clone, Eq, PartialEq)]
pub struct ExecutedBlock {
    /// Block data that cannot be regenerated.
    block: Block,
    /// The state_compute_result is calculated for all the pending blocks prior to insertion to
    /// the tree. The execution results are not persisted: they're recalculated again for the
    /// pending blocks upon restart.
    state_compute_result: StateComputeResult,
}

impl Debug for ExecutedBlock {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for ExecutedBlock {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.block())
    }
}

impl ExecutedBlock {
    pub fn new(block: Block, state_compute_result: StateComputeResult) -> Self {
        Self {
            block,
            state_compute_result,
        }
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn id(&self) -> HashValue {
        self.block().id()
    }

    pub fn epoch(&self) -> u64 {
        self.block.epoch()
    }

    pub fn payload(&self) -> Option<&Payload> {
        self.block().payload()
    }

    pub fn parent_id(&self) -> HashValue {
        self.quorum_cert().certified_block().id()
    }

    pub fn quorum_cert(&self) -> &QuorumCert {
        self.block().quorum_cert()
    }

    pub fn round(&self) -> Round {
        self.block().round()
    }

    pub fn timestamp_usecs(&self) -> u64 {
        self.block().timestamp_usecs()
    }

    pub fn compute_result(&self) -> &StateComputeResult {
        &self.state_compute_result
    }

    pub fn block_info(&self) -> BlockInfo {
        self.block().gen_block_info(
            self.compute_result().root_hash(),
            self.compute_result().version(),
            self.compute_result().epoch_state().clone(),
        )
    }

    pub fn maybe_signed_vote_proposal(&self, decoupled_execution: bool) -> MaybeSignedVoteProposal {
        MaybeSignedVoteProposal {
            vote_proposal: VoteProposal::new(
                self.compute_result().extension_proof(),
                self.block.clone(),
                self.compute_result().epoch_state().clone(),
                decoupled_execution,
            ),
            signature: self.compute_result().signature().clone(),
        }
    }

    pub fn transactions_to_commit(&self) -> Vec<Transaction> {
        // reconfiguration suffix don't execute
        if self.is_reconfiguration_suffix() {
            return vec![];
        }
        itertools::zip_eq(
            self.block.transactions_to_execute(),
            self.state_compute_result.compute_status(),
        )
        .filter_map(|(txn, status)| match status {
            TransactionStatus::Keep(_) => Some(txn),
            _ => None,
        })
        .collect()
    }

    pub fn reconfig_event(&self) -> Vec<ContractEvent> {
        // reconfiguration suffix don't count, the state compute result is carried over from parents
        if self.is_reconfiguration_suffix() {
            return vec![];
        }
        self.state_compute_result.reconfig_events().to_vec()
    }

    /// The block is suffix of a reconfiguration block if the state result carries over the epoch state
    /// from parent but has no transaction.
    pub fn is_reconfiguration_suffix(&self) -> bool {
        self.state_compute_result.has_reconfiguration()
            && self.state_compute_result.compute_status().is_empty()
    }
}
