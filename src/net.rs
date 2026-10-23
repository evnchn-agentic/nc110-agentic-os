//! smoltcp `phy::Device` over the Realtek C+ driver (rtl8139 C+ or RTL8101E) (polled, one TX descriptor, NRX RX descriptors).
use crate::nic::{self, Bus, Rtl};
use crate::nic_regs::Regs;
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

pub struct Nic(pub Rtl);

/// A smoltcp device an SSH session can run on: the Ethernet NIC (which bmctl's chain-load and sleep also need) or
/// the WiFi data path (iwl::WifiDev, no Rtl).
pub trait Link: Device {
    fn rtl(&self) -> Option<&Rtl> {
        None
    }
}

impl Link for Nic {
    fn rtl(&self) -> Option<&Rtl> {
        Some(&self.0)
    }
}

/// Frames `receive` may hand out before the next `rx_budget` call. None = unlimited (plain SSH never sets it);
/// bmctl sets Some(32) (Some(64) in `bmctl ssh`) before every poll so a flood cannot starve its deadline checks.
static mut RX_BUDGET: Option<u32> = None;

pub fn rx_budget(n: Option<u32>) {
    unsafe { RX_BUDGET = n };
}

pub struct Rx {
    idx: usize,
    len: usize,
    regs: &'static Regs,
}
pub struct Tx {
    bus: Bus,
    regs: &'static Regs,
}

impl Drop for Rx {
    fn drop(&mut self) {
        nic::rx_release(self.regs, self.idx);
    }
}

impl RxToken for Rx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(nic::rx_buf(self.idx, self.len)) // descriptor goes back to the NIC when `self` drops
    }
}

impl TxToken for Tx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        nic::tx_with(self.bus, self.regs, len, f)
    }
}

impl Device for Nic {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx;

    fn receive(&mut self, _t: Instant) -> Option<(Rx, Tx)> {
        unsafe {
            match RX_BUDGET {
                Some(0) => return None,
                Some(n) => RX_BUDGET = Some(n - 1),
                None => {}
            }
        }
        let (idx, len) = self.0.rx_poll()?;
        Some((Rx { idx, len, regs: self.0.regs }, Tx { bus: self.0.bus, regs: self.0.regs }))
    }

    fn transmit(&mut self, _t: Instant) -> Option<Tx> {
        Some(Tx { bus: self.0.bus, regs: self.0.regs })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ethernet;
        c.max_transmission_unit = 1514;
        c.max_burst_size = Some(1);
        c
    }
}
