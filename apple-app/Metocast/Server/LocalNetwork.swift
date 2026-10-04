#if os(macOS)
import Foundation
import SystemConfiguration

/// How other devices on the network reach this Mac. Mirrors `get_local_host` in
/// `src-tauri/src/commands/server.rs`.
enum LocalNetwork {
    /// The Bonjour name, e.g. `amacmini.local`, which survives DHCP lease changes;
    /// otherwise the first LAN IPv4 address.
    static var hostName: String? {
        if let name = SCDynamicStoreCopyLocalHostName(nil) as String?, !name.isEmpty {
            return "\(name).local"
        }
        return ipv4Address
    }

    private static var ipv4Address: String? {
        var addresses: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&addresses) == 0, let first = addresses else { return nil }
        defer { freeifaddrs(addresses) }
        for pointer in sequence(first: first, next: { $0.pointee.ifa_next }) {
            let interface = pointer.pointee
            let flags = Int32(interface.ifa_flags)
            guard let address = interface.ifa_addr,
                  address.pointee.sa_family == UInt8(AF_INET),
                  flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0
            else { continue }
            var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            let result = getnameinfo(
                address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST
            )
            if result == 0 {
                let bytes = host.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
                return String(decoding: bytes, as: UTF8.self)
            }
        }
        return nil
    }
}
#endif
