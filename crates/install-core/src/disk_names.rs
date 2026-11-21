//! Device paths of a disk and its partitions, as the kernel names them.

/// Whether `path` names a device under `/dev`.
pub fn is_disk_path(path: &str) -> bool {
    path.strip_prefix("/dev/")
        .is_some_and(|name| !name.is_empty() && !name.contains("..") && !name.ends_with('/'))
}

/// The path of partition `number` of `device`: `/dev/vda` gives `/dev/vda1`, and a device
/// whose name ends in a digit, such as `/dev/nvme0n1`, gives `/dev/nvme0n1p1`.
pub fn partition_path(device: &str, number: u32) -> String {
    if device.ends_with(|c: char| c.is_ascii_digit()) {
        format!("{device}p{number}")
    } else {
        format!("{device}{number}")
    }
}

/// The partition number of `partition` when it names a partition of `device`.
pub fn partition_number(device: &str, partition: &str) -> Option<u32> {
    let rest = partition.strip_prefix(device)?;
    let digits = if device.ends_with(|c: char| c.is_ascii_digit()) {
        rest.strip_prefix('p')?
    } else {
        rest
    };
    if digits.is_empty() || digits.starts_with('0') || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_paths_follow_the_kernel_names() {
        assert_eq!(partition_path("/dev/vda", 3), "/dev/vda3");
        assert_eq!(partition_path("/dev/nvme0n1", 2), "/dev/nvme0n1p2");
        assert_eq!(partition_path("/dev/mmcblk0", 1), "/dev/mmcblk0p1");
    }

    #[test]
    fn only_partitions_of_the_device_have_a_number() {
        assert_eq!(partition_number("/dev/vda", "/dev/vda12"), Some(12));
        assert_eq!(partition_number("/dev/nvme0n1", "/dev/nvme0n1p3"), Some(3));
        assert_eq!(partition_number("/dev/vda", "/dev/vdb1"), None);
        assert_eq!(partition_number("/dev/vda", "/dev/vda"), None);
        assert_eq!(partition_number("/dev/nvme0n1", "/dev/nvme0n12"), None);
        assert_eq!(partition_number("/dev/sd", "/dev/sda1"), None);
    }
}
