import { Panproto, SymmetricLensHandle } from '@panproto/core';

const panproto = await Panproto.init();

// Two teams with different schemas — neither is "old" or "new"
const teamA = buildTeamASchema(panproto);
const teamB = buildTeamBSchema(panproto);

// Auto-generate a symmetric lens
using sync = SymmetricLensHandle.fromSchemas(teamA, teamB, panproto._wasm);

// Sync A's data to B's view
const { view: bView, complement: bComplement } = sync.syncLeftToRight(
  teamAData, teamAComplement
);

// Sync B's data to A's view
const { view: aView, complement: aComplement } = sync.syncRightToLeft(
  teamBData, teamBComplement
);
