use crate::architecture::arm::{
    ap::{MemoryAP},
    memory::romtable::{RomTable},
    memory::ADIMemoryInterface,
    ArmCommunicationInterface,
};
use crate::config::{
    MemoryRegion, RawFlashAlgorithm, RegistryError, Target, TargetSelector,
};
use crate::core::CoreType;
use crate::{Core, CoreList, Error, Memory, MemoryList, Probe};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Session {
    inner: Rc<RefCell<InnerSession>>,
}

struct InnerSession {
    target: Target,
    architecture_session: ArchitectureSession,
}

enum ArchitectureSession {
    Arm(ArmCommunicationInterface),
}

impl Session {
    /// Open a new session with a given debug target
    pub fn new(probe: Probe, target: impl Into<TargetSelector>) -> Result<Self, Error> {
        // TODO: Handle different architectures

        let arm_interface = ArmCommunicationInterface::new(probe);

        let target = target.into();
        let target = match target.into() {
            TargetSelector::Unspecified(name) => {
                match crate::config::registry::get_target_by_name(name) {
                    Ok(target) => target,
                    Err(err) => return Err(err)?,
                }
            }
            TargetSelector::Specified(target) => target,
            TargetSelector::Auto => {
                let arm_chip = None;
                // TODO: Replace this with a generic core!
                // let arm_chip = ArmChipInfo::read_from_rom_table(core, &mut arm_interface)
                //     .map(|option| option.map(ChipInfo::Arm))?;
                if let Some(chip) = arm_chip {
                    match crate::config::registry::get_target_by_chip_info(chip) {
                        Ok(target) => target,
                        Err(err) => return Err(err)?,
                    }
                } else {
                    // Not sure if this is ok.
                    return Err(Error::ChipNotFound(RegistryError::ChipAutodetectFailed));
                }
            }
        };

        let session = ArchitectureSession::Arm(arm_interface);

        Ok(Self {
            inner: Rc::new(RefCell::new(InnerSession {
                target,
                architecture_session: session,
            })),
        })
    }

    pub fn list_cores(&self) -> CoreList {
        CoreList::new(vec![self.inner.borrow().target.core_type.clone()])
    }

    pub fn attach_to_core(&self, n: usize) -> Result<Core, Error> {
        let core = self
            .list_cores()
            .get(n)
            .ok_or_else(|| Error::CoreNotFound(n))?
            .attach(self.clone(), self.attach_to_memory(n)?, n);
        match self.inner.borrow_mut().architecture_session {
            ArchitectureSession::Arm(ref mut _interface) => {
                Ok(core)
            }
        }
    }

    pub fn attach_to_specific_core(&self, core_type: CoreType) -> Result<Core, Error> {
        let core = core_type.attach(self.clone(), self.attach_to_memory(0)?, 0);
        Ok(core)
    }

    pub fn attach_to_core_with_specific_memory(
        &self,
        n: usize,
        memory: Option<Memory>,
    ) -> Result<Core, Error> {
        let core = self
            .list_cores()
            .get(n)
            .ok_or_else(|| Error::CoreNotFound(n))?
            .attach(
                self.clone(),
                match memory {
                    Some(memory) => memory,
                    None => self.attach_to_memory(0)?,
                },
                n,
            );
        Ok(core)
    }

    pub fn list_memories(&self) -> MemoryList {
        MemoryList::new(vec![])
    }

    pub fn attach_to_memory(&self, id: usize) -> Result<Memory, Error> {
        match self.inner.borrow_mut().architecture_session {
            ArchitectureSession::Arm(ref mut interface) => {
                if let Some(memory) = interface.dedicated_memory_interface() {
                    Ok(memory)
                } else {
                    // TODO: Change this to actually grab the proper memory IF.
                    // For now always use the ARM IF.
                    let maps = interface.memory_access_ports()?;
                    Ok(Memory::new(
                        ADIMemoryInterface::<ArmCommunicationInterface>::new(
                            interface.clone(),
                            maps[id].id(),
                        ),
                    ))
                }
            }
        }
    }

    pub fn attach_to_best_memory(&self) -> Result<Memory, Error> {
        match self.inner.borrow().architecture_session {
            ArchitectureSession::Arm(ref interface) => {
                if let Some(memory) = interface.dedicated_memory_interface() {
                    Ok(memory)
                } else {
                    // TODO: Change this to actually grab the proper memory IF.
                    // For now always use the ARM IF.
                    Ok(Memory::new(
                        ADIMemoryInterface::<ArmCommunicationInterface>::new(interface.clone(), 0),
                    ))
                }
            }
        }
    }

    pub fn flash_algorithms(&self) -> Vec<RawFlashAlgorithm> {
        self.inner.borrow().target.flash_algorithms.clone()
    }

    pub fn memory_map(&self) -> Vec<MemoryRegion> {
        self.inner.borrow().target.memory_map.clone()
    }

    pub fn read_swv(&self) -> Result<Vec<u8>, Error> {
        match &mut self.inner.borrow_mut().architecture_session {
            ArchitectureSession::Arm(interface) => interface.read_swv(),
        }
    }

    pub fn setup_tracing(&mut self, core: &mut Core) -> Result<(), Error> {
        match self.inner.borrow_mut().architecture_session {
            ArchitectureSession::Arm(ref mut interface) => {
                println!("Setting up tracing");
                let maps = interface.memory_access_ports()?;
                println!("{:?}", maps);

                let baseaddr = maps[core.id()].base_address();
                println!("{:x?}", baseaddr);
                
                let rom_table = RomTable::try_parse(core, baseaddr as u64)
                    .map_err(Error::architecture_specific)?;

                for e in rom_table.entries() {
                    println!(
                        "ROM Table Entry: Component @ 0x{:08x}",
                        e.component_id.component_address()
                    );
                }

                crate::architecture::arm::component::setup_tracing(core, &rom_table)
            }
        }
    }

    // pub fn start_trace_memory_address(core: &mut Core, romtable: &RomTable, addr: u32) -> Result<(), Error> {

    // }
}
