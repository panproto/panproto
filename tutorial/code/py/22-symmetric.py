from panproto import Panproto, SymmetricLensHandle

panproto = Panproto.init()

with SymmetricLensHandle.from_schemas(team_a, team_b, panproto._wasm) as sync:
    b_view, b_comp = sync.sync_left_to_right(a_data, a_complement)
    a_view, a_comp = sync.sync_right_to_left(b_data, b_complement)
