use crate::ProcessIdentity;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[must_use]
pub fn fingerprint_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

#[must_use]
pub fn fingerprint_process_identity(identity: &ProcessIdentity) -> String {
    let row = format!(
        "{}:{}:{}:{}",
        identity.pid,
        identity.started_at_unix_micros,
        identity.executable_device.unwrap_or_default(),
        identity.executable_inode.unwrap_or_default()
    );
    fingerprint_parts([row.as_bytes()])
}

#[must_use]
pub fn fingerprint_process_set<'a>(
    identities: impl IntoIterator<Item = &'a ProcessIdentity>,
) -> String {
    let mut rows = identities
        .into_iter()
        .map(|identity| {
            format!(
                "{}:{}:{}:{}",
                identity.pid,
                identity.started_at_unix_micros,
                identity.executable_device.unwrap_or_default(),
                identity.executable_inode.unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    rows.sort_unstable();
    fingerprint_parts(rows.iter().map(String::as_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_order_independent_for_process_sets() {
        let a = ProcessIdentity {
            pid: 10,
            started_at_unix_micros: 20,
            executable_device: Some(1),
            executable_inode: Some(2),
        };
        let b = ProcessIdentity {
            pid: 11,
            started_at_unix_micros: 21,
            executable_device: Some(1),
            executable_inode: Some(3),
        };
        assert_eq!(
            fingerprint_process_set([&a, &b]),
            fingerprint_process_set([&b, &a])
        );
    }

    #[test]
    fn process_identity_fingerprint_covers_every_exact_identity_field() {
        let baseline = ProcessIdentity {
            pid: 10,
            started_at_unix_micros: 20,
            executable_device: Some(30),
            executable_inode: Some(40),
        };

        for changed in [
            ProcessIdentity {
                pid: 11,
                ..baseline.clone()
            },
            ProcessIdentity {
                started_at_unix_micros: 21,
                ..baseline.clone()
            },
            ProcessIdentity {
                executable_device: Some(31),
                ..baseline.clone()
            },
            ProcessIdentity {
                executable_inode: Some(41),
                ..baseline.clone()
            },
        ] {
            assert_ne!(
                fingerprint_process_identity(&baseline),
                fingerprint_process_identity(&changed)
            );
        }
    }
}
