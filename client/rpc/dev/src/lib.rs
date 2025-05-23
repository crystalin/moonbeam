// Copyright 2019-2025 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use cumulus_primitives_core::ParaId;
use cumulus_primitives_core::XcmpMessageFormat;
use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::{
		error::{INTERNAL_ERROR_CODE, INTERNAL_ERROR_MSG},
		ErrorObjectOwned,
	},
};
use parity_scale_codec::Encode;
use xcm::opaque::lts::Weight;
use xcm::v5::prelude::*;
use xcm_primitives::DEFAULT_PROOF_SIZE;

/// This RPC interface is used to provide methods in dev mode only
#[rpc(server)]
#[jsonrpsee::core::async_trait]
pub trait DevApi {
	/// Inject a downward xcm message - A message that comes from the relay chain.
	/// You may provide an arbitrary message, or if you provide an empty byte array,
	/// Then a default message (DOT transfer down to ALITH) will be injected
	#[method(name = "xcm_injectDownwardMessage")]
	async fn inject_downward_message(&self, message: Vec<u8>) -> RpcResult<()>;

	/// Inject an HRMP message - A message that comes from a dedicated channel to a sibling
	/// parachain.
	///
	/// Cumulus Parachain System seems to have a constraint that at most one hrmp message will be
	/// sent on a channel per block. At least that's what this comment implies:
	/// https://github.com/paritytech/cumulus/blob/c308c01b/pallets/parachain-system/src/lib.rs#L204
	/// Neither this RPC, nor the mock inherent data provider make any attempt to enforce this
	/// constraint. In fact, violating it may be useful for testing.
	/// The method accepts a sending paraId and a bytearray representing an arbitrary message as
	/// parameters. If you provide an emtpy byte array, then a default message representing a
	/// transfer of the sending paraId's native token will be injected.
	#[method(name = "xcm_injectHrmpMessage")]
	async fn inject_hrmp_message(&self, sender: ParaId, message: Vec<u8>) -> RpcResult<()>;

	/// Skip N relay blocks, for testing purposes
	#[method(name = "test_skipRelayBlocks")]
	async fn skip_relay_blocks(&self, n: u32) -> RpcResult<()>;
}

pub struct DevRpc {
	pub downward_message_channel: flume::Sender<Vec<u8>>,
	pub hrmp_message_channel: flume::Sender<(ParaId, Vec<u8>)>,
	pub additional_relay_offset: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

#[jsonrpsee::core::async_trait]
impl DevApiServer for DevRpc {
	async fn inject_downward_message(&self, msg: Vec<u8>) -> RpcResult<()> {
		let downward_message_channel = self.downward_message_channel.clone();
		// If no message is supplied, inject a default one.
		let msg = if msg.is_empty() {
			xcm::VersionedXcm::<()>::V5(Xcm(vec![
				ReserveAssetDeposited((Parent, 10000000000000u128).into()),
				ClearOrigin,
				BuyExecution {
					fees: (Parent, 10000000000000u128).into(),
					weight_limit: Limited(Weight::from_parts(
						10_000_000_000u64,
						DEFAULT_PROOF_SIZE,
					)),
				},
				DepositAsset {
					assets: AllCounted(1).into(),
					beneficiary: Location::new(
						0,
						[AccountKey20 {
							network: None,
							key: hex_literal::hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"),
						}],
					),
				},
			]))
			.encode()
		} else {
			msg
		};

		// Push the message to the shared channel where it will be queued up
		// to be injected into an upcoming block.
		downward_message_channel
			.send_async(msg)
			.await
			.map_err(|err| internal_err(err.to_string()))?;

		Ok(())
	}

	async fn inject_hrmp_message(&self, sender: ParaId, msg: Vec<u8>) -> RpcResult<()> {
		let hrmp_message_channel = self.hrmp_message_channel.clone();

		// If no message is supplied, inject a default one.
		let msg = if msg.is_empty() {
			let mut mes = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
			mes.append(
				&mut (xcm::VersionedXcm::<()>::V5(Xcm(vec![
					ReserveAssetDeposited(
						((Parent, Parachain(sender.into())), 10000000000000u128).into(),
					),
					ClearOrigin,
					BuyExecution {
						fees: ((Parent, Parachain(sender.into())), 10000000000000u128).into(),
						weight_limit: Limited(Weight::from_parts(
							10_000_000_000u64,
							DEFAULT_PROOF_SIZE,
						)),
					},
					DepositAsset {
						assets: AllCounted(1).into(),
						beneficiary: Location::new(
							0,
							[AccountKey20 {
								network: None,
								key: hex_literal::hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"),
							}],
						),
					},
				]))
				.encode()),
			);
			mes
		} else {
			msg
		};

		// Push the message to the shared channel where it will be queued up
		// to be injected into an upcoming block.
		hrmp_message_channel
			.send_async((sender, msg))
			.await
			.map_err(|err| internal_err(err.to_string()))?;

		Ok(())
	}

	async fn skip_relay_blocks(&self, n: u32) -> RpcResult<()> {
		self.additional_relay_offset
			.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
		Ok(())
	}
}

// This bit cribbed from frontier.
pub fn internal_err<T: ToString>(message: T) -> ErrorObjectOwned {
	ErrorObjectOwned::owned(
		INTERNAL_ERROR_CODE,
		INTERNAL_ERROR_MSG,
		Some(message.to_string()),
	)
}
