//! The disk screen: the disks with their model and size, the four disk choices for the
//! chosen disk, with the unavailable ones shown with their reason, and the system size.
use iced::{
    Element, Length,
    widget::{Row as Line, button, column, scrollable, slider, text},
};
use inayos_install_core::{
    Choice, Disk,
    inventory::{DiskInfo, Offer, SizeLimits},
    sizes::GIB,
};

use super::{Field, Focus, Key, Reply, list};
use crate::theme;

/// The system sizes offered as one click each, in GiB.
pub const PRESETS: [u64; 4] = [50, 100, 150, 200];
/// The system size chosen until the user changes it, in GiB.
pub const DEFAULT_GIB: u64 = 150;

/// The choices from the one that keeps the most to the one that erases the whole disk.
const SAFEST_FIRST: [Choice; 4] = [
    Choice::ReinstallKeepHome,
    Choice::ShrinkExt4KeepHome,
    Choice::BesideWindows,
    Choice::WipeDisk,
];

const DISKS: &str = "disks";
const OFFERS: &str = "offers";
const SIZES: &str = "sizes";
const FIELDS: [Field; 3] = [Field::List(DISKS), Field::List(OFFERS), Field::List(SIZES)];

pub struct State {
    /// The disks, or why they could not be read.
    disks: Result<Vec<DiskInfo>, String>,
    disk: Option<usize>,
    /// The chosen offer of the chosen disk, always an available one.
    offer: Option<usize>,
    size_gib: u64,
    focus: Focus,
}

#[derive(Clone, Debug)]
pub enum Message {
    Disk(usize),
    Offer(usize),
    Size(u32),
}

impl State {
    /// Starts on the first disk and its safest available choice.
    pub fn new(disks: Result<Vec<DiskInfo>, String>) -> Self {
        let mut state = State {
            disks,
            disk: None,
            offer: None,
            size_gib: DEFAULT_GIB,
            focus: Focus::default(),
        };
        if state.disk_list().is_some_and(|disks| !disks.is_empty()) {
            state.choose_disk(0);
        }
        state
    }

    fn disk_list(&self) -> Option<&[DiskInfo]> {
        self.disks.as_deref().ok()
    }

    /// The chosen disk.
    pub fn disk(&self) -> Option<&DiskInfo> {
        self.disk_list()?.get(self.disk?)
    }

    fn chosen_offer(&self) -> Option<&Offer> {
        self.disk()?.offers.get(self.offer?)
    }

    /// The size limits of the chosen choice, when it takes a size.
    fn limits(&self) -> Option<SizeLimits> {
        self.chosen_offer()?.system_size
    }

    /// Chooses the disk and preselects its safest available choice, so that erasing the
    /// whole disk is never what Enter keeps when the disk holds files that can stay.
    fn choose_disk(&mut self, index: usize) {
        self.disk = Some(index);
        self.offer = self.disk().and_then(|disk| {
            SAFEST_FIRST.iter().find_map(|choice| {
                disk.offers
                    .iter()
                    .position(|offer| offer.choice == *choice && offer.available.is_ok())
            })
        });
        self.fit_size();
    }

    fn choose_offer(&mut self, index: usize) {
        if self
            .disk()
            .and_then(|disk| disk.offers.get(index))
            .is_some_and(|offer| offer.available.is_ok())
        {
            self.offer = Some(index);
            self.fit_size();
        }
    }

    fn fit_size(&mut self) {
        if let Some(limits) = self.limits() {
            self.size_gib = limits.clamp(self.size_gib);
        }
    }

    /// The presets the chosen choice allows, each moved into its limits, without repeats.
    fn presets(&self) -> Vec<u64> {
        let Some(limits) = self.limits() else {
            return Vec::new();
        };
        let mut presets: Vec<u64> = PRESETS.iter().map(|gib| limits.clamp(*gib)).collect();
        presets.dedup();
        presets
    }

    /// The disk answer for the chosen disk, choice and size. None until an available choice
    /// is chosen.
    pub fn answer(&self) -> Option<Disk> {
        let disk = self.disk()?;
        let offer = self.chosen_offer()?;
        let partitions = offer.available.as_ref().ok()?;
        let device = disk.path.clone();
        Some(match offer.choice {
            Choice::WipeDisk => Disk::WipeDisk {
                device,
                system_size_gib: self.size_gib,
            },
            Choice::ReinstallKeepHome => Disk::ReinstallKeepHome {
                device,
                efi_partition: partitions.efi.clone()?,
                system_partition: partitions.system.clone()?,
                home_partition: partitions.home.clone()?,
            },
            Choice::ShrinkExt4KeepHome => Disk::ShrinkExt4KeepHome {
                device,
                system_size_gib: self.size_gib,
                efi_partition: partitions.efi.clone()?,
                home_partition: partitions.home.clone()?,
            },
            Choice::BesideWindows => Disk::BesideWindows { device },
        })
    }

    pub fn update(&mut self, message: Message) -> Reply<Message> {
        match message {
            Message::Disk(index) => {
                self.focus.set(&FIELDS, Field::List(DISKS));
                self.choose_disk(index);
            }
            Message::Offer(index) => {
                self.focus.set(&FIELDS, Field::List(OFFERS));
                self.choose_offer(index);
            }
            Message::Size(gib) => {
                self.focus.set(&FIELDS, Field::List(SIZES));
                self.size_gib = u64::from(gib);
                self.fit_size();
            }
        }
        Reply::none()
    }

    pub fn key(&mut self, key: Key) -> Reply<Message> {
        let by = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::Space => return Reply::none(),
            Key::Next | Key::Previous => {
                return Reply::task(self.focus.step(&FIELDS, key == Key::Next));
            }
        };
        match self.focus.field(&FIELDS) {
            Some(Field::List(DISKS)) => {
                let count = self.disk_list().map_or(0, <[DiskInfo]>::len);
                if let Some(index) = list::moved(self.disk, count, by) {
                    self.choose_disk(index);
                    return Reply::task(list::reveal(DISKS, index));
                }
            }
            Some(Field::List(OFFERS)) => {
                if let Some(index) = self.next_available(by) {
                    self.offer = Some(index);
                    self.fit_size();
                }
            }
            Some(Field::List(SIZES)) => {
                let presets = self.presets();
                let at = presets.iter().position(|gib| *gib == self.size_gib);
                let at = match at {
                    Some(at) => list::moved(Some(at), presets.len(), by),
                    // A size between presets moves to the nearest preset in that direction.
                    None if by > 0 => presets.iter().position(|gib| *gib > self.size_gib),
                    None => presets.iter().rposition(|gib| *gib < self.size_gib),
                };
                if let Some(gib) = at.and_then(|at| presets.get(at)) {
                    self.size_gib = *gib;
                }
            }
            _ => {}
        }
        Reply::none()
    }

    /// The next available choice from the chosen one, `by` steps in its direction.
    fn next_available(&self, by: isize) -> Option<usize> {
        let offers = &self.disk()?.offers;
        let available: Vec<usize> = (0..offers.len())
            .filter(|index| offers[*index].available.is_ok())
            .collect();
        let at = self
            .offer
            .and_then(|offer| available.iter().position(|index| *index == offer));
        list::moved(at, available.len(), by).map(|at| available[at])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let focused = self.focus.field(&FIELDS);
        let disks = match &self.disks {
            Err(reason) => {
                return text(format!("The disks cannot be read: {reason}"))
                    .color(theme::DANGER)
                    .into();
            }
            Ok(disks) if disks.is_empty() => {
                return text("This computer has no disk InayOS can install on.")
                    .color(theme::DANGER)
                    .into();
            }
            Ok(disks) => disks,
        };
        let disk_rows = disks
            .iter()
            .map(|disk| list::Row::new(describe_disk(disk)))
            .collect();
        let mut page = column![
            text("Disk").size(theme::TEXT + 4).color(theme::PINK),
            list::view(
                DISKS,
                disk_rows,
                self.disk,
                focused == Some(Field::List(DISKS)),
                Length::Shrink,
                Message::Disk,
            ),
            text("What to do with it")
                .size(theme::TEXT + 4)
                .color(theme::PINK),
        ]
        .spacing(12);
        if let Some(disk) = self.disk() {
            let offer_rows = disk
                .offers
                .iter()
                .map(|offer| match &offer.available {
                    Ok(_) => list::Row::new(choice_label(offer.choice)),
                    Err(reason) => list::Row {
                        label: format!("{}: {reason}", choice_label(offer.choice)),
                        enabled: false,
                    },
                })
                .collect();
            page = page.push(list::view(
                OFFERS,
                offer_rows,
                self.offer,
                focused == Some(Field::List(OFFERS)),
                Length::Shrink,
                Message::Offer,
            ));
        }
        if let Some(limits) = self.limits() {
            page = page.push(self.size_view(limits, focused == Some(Field::List(SIZES))));
        }
        // The page scrolls, so a small screen or the on-screen keyboard hides nothing for
        // good.
        scrollable(page).height(Length::Fill).into()
    }

    fn size_view(&self, limits: SizeLimits, focused: bool) -> Element<'_, Message> {
        let range = to_u32(limits.min_gib)..=to_u32(limits.max_gib);
        let mut presets = Line::new().spacing(12);
        for gib in self.presets() {
            presets = presets.push(
                button(text(format!("{gib} GiB")).size(theme::TEXT))
                    .padding(theme::PAD)
                    .style(theme::row(gib == self.size_gib, focused))
                    .on_press(Message::Size(to_u32(gib))),
            );
        }
        column![
            text(format!("System size: {} GiB", self.size_gib))
                .size(theme::TEXT + 4)
                .color(theme::PINK),
            text(format!(
                "From {} to {} GiB. The rest of the space is home, for your files.",
                limits.min_gib, limits.max_gib
            ))
            .color(theme::DIM),
            slider(range, to_u32(self.size_gib), Message::Size).height(32),
            presets,
        ]
        .spacing(12)
        .into()
    }
}

fn to_u32(gib: u64) -> u32 {
    u32::try_from(gib).unwrap_or(u32::MAX)
}

/// A size in bytes as GiB with one decimal, such as `476.9 GiB`.
pub fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

/// The disk as the list and the summary show it: path, model and size.
pub fn describe_disk(disk: &DiskInfo) -> String {
    if disk.model.is_empty() {
        format!("{}, {}", disk.path, gib(disk.size_bytes))
    } else {
        format!("{}, {}, {}", disk.path, disk.model, gib(disk.size_bytes))
    }
}

pub fn choice_label(choice: Choice) -> &'static str {
    match choice {
        Choice::WipeDisk => "Erase the whole disk and install InayOS",
        Choice::ReinstallKeepHome => "Reinstall InayOS and keep the home partition",
        Choice::ShrinkExt4KeepHome => "Shrink the Linux partition and keep it as home",
        Choice::BesideWindows => "Install beside Windows",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use inayos_install_core::{
        Choice, Disk,
        inventory::{DiskInfo, Offer, Partitions, SizeLimits},
    };

    use super::{Key, Message, State};

    const LIMITS: SizeLimits = SizeLimits {
        min_gib: 20,
        max_gib: 120,
    };

    fn offer(choice: Choice, available: Result<Partitions, &str>) -> Offer {
        let takes_size = matches!(choice, Choice::WipeDisk | Choice::ShrinkExt4KeepHome);
        Offer {
            choice,
            system_size: (takes_size && available.is_ok()).then_some(LIMITS),
            available: available.map_err(str::to_owned),
        }
    }

    /// A 128 GiB disk with an earlier InayOS on it and an ext4 partition that can shrink.
    pub(crate) fn disk() -> DiskInfo {
        let part = |n: u8| Some(format!("/dev/vda{n}"));
        DiskInfo {
            path: "/dev/vda".into(),
            model: "QEMU HARDDISK".into(),
            size_bytes: 128 << 30,
            offers: vec![
                offer(Choice::WipeDisk, Ok(Partitions::default())),
                offer(
                    Choice::ReinstallKeepHome,
                    Ok(Partitions {
                        efi: part(1),
                        system: part(2),
                        home: part(3),
                    }),
                ),
                offer(
                    Choice::ShrinkExt4KeepHome,
                    Ok(Partitions {
                        efi: part(1),
                        system: None,
                        home: part(3),
                    }),
                ),
                offer(Choice::BesideWindows, Err("comes in a later version")),
            ],
        }
    }

    #[test]
    fn a_disk_that_holds_files_that_can_stay_does_not_start_on_erasing_it() {
        let state = State::new(Ok(vec![disk()]));
        assert!(matches!(
            state.answer(),
            Some(Disk::ReinstallKeepHome { .. })
        ));
        let mut no_reinstall = disk();
        no_reinstall.offers[1].available = Err("no earlier InayOS".into());
        assert!(matches!(
            State::new(Ok(vec![no_reinstall])).answer(),
            Some(Disk::ShrinkExt4KeepHome { .. })
        ));
    }

    #[test]
    fn each_available_choice_gives_the_disk_answer_with_its_partitions_and_size() {
        let mut state = State::new(Ok(vec![disk()]));
        state.update(Message::Offer(0));
        assert_eq!(
            state.answer(),
            Some(Disk::WipeDisk {
                device: "/dev/vda".into(),
                system_size_gib: 120,
            }),
            "the default 150 GiB is moved into the disk's limits"
        );
        state.update(Message::Offer(1));
        assert_eq!(
            state.answer(),
            Some(Disk::ReinstallKeepHome {
                device: "/dev/vda".into(),
                efi_partition: "/dev/vda1".into(),
                system_partition: "/dev/vda2".into(),
                home_partition: "/dev/vda3".into(),
            })
        );
        state.update(Message::Offer(2));
        state.update(Message::Size(50));
        assert_eq!(
            state.answer(),
            Some(Disk::ShrinkExt4KeepHome {
                device: "/dev/vda".into(),
                system_size_gib: 50,
                efi_partition: "/dev/vda1".into(),
                home_partition: "/dev/vda3".into(),
            })
        );
    }

    #[test]
    fn an_unavailable_choice_cannot_be_chosen_by_click_or_key() {
        let mut state = State::new(Ok(vec![disk()]));
        state.update(Message::Offer(2));
        state.update(Message::Offer(3));
        assert!(matches!(
            state.answer(),
            Some(Disk::ShrinkExt4KeepHome { .. })
        ));
        state.key(Key::Next);
        state.key(Key::Down);
        assert!(matches!(
            state.answer(),
            Some(Disk::ShrinkExt4KeepHome { .. })
        ));
    }

    #[test]
    fn up_and_down_on_the_size_step_through_the_presets_inside_the_limits() {
        let mut state = State::new(Ok(vec![disk()]));
        state.update(Message::Offer(0));
        state.key(Key::Next);
        let size = |state: &State| match state.answer() {
            Some(Disk::WipeDisk {
                system_size_gib, ..
            }) => system_size_gib,
            other => panic!("unexpected answer {other:?}"),
        };
        assert_eq!(size(&state), 120);
        state.key(Key::Up);
        assert_eq!(size(&state), 100);
        state.key(Key::Up);
        state.key(Key::Up);
        state.key(Key::Up);
        assert_eq!(size(&state), 50);
    }

    #[test]
    fn a_disk_without_available_choice_gives_no_answer() {
        let mut full = disk();
        for offer in &mut full.offers {
            offer.available = Err("in use".into());
            offer.system_size = None;
        }
        assert_eq!(State::new(Ok(vec![full])).answer(), None);
    }
}
