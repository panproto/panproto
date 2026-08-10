import CPanproto
import Foundation
import PanprotoFFI

@main
struct Smoke {
    static func main() {
        print("pp_init:", RawStatus(code: pp_init()))
        let listed = withPpOutBuffer { pp_registry_list_builtin($0) }
        print("list_builtin:", listed.status, "bytes:", listed.bytes.count)
        print("first bytes:", Array(listed.bytes.prefix(32)))
    }
}
