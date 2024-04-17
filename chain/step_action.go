// Copyright (C) 2024, Ava Labs, Inc. All rights reserved.
// See the file LICENSE for licensing terms.

package chain

import (
	"context"

	"github.com/ava-labs/avalanchego/ids"
	"github.com/ava-labs/avalanchego/vms/platformvm/warp"
	"github.com/ava-labs/hypersdk/codec"
	"github.com/ava-labs/hypersdk/state"
)

type StepAction struct {}

// TODO define a function with parameters
func NewStepAction() *StepAction {
	action := &StepAction{}
	return action
}

func (s *StepAction) MaxComputeUnits(rules Rules) uint64 {
	return 0
}

func (s *StepAction) StateKeysMaxChunks() []uint16 {
	return []uint16{}
}

func (s *StepAction) StateKeys(actor codec.Address, txID ids.ID) state.Keys {
	return state.Keys{}
}

func (s *StepAction) Execute(
		ctx context.Context,
		r Rules,
		mu state.Mutable,
		timestamp int64,
		actor codec.Address,
		txID ids.ID,
		warpVerified bool,
	) (success bool, computeUnits uint64, output []byte, warpMessage *warp.UnsignedMessage, err error) {
		return true, 0, []byte{}, &warp.UnsignedMessage{}, nil
}

func (s *StepAction) OutputsWarpMessage() bool {
	return true
}
