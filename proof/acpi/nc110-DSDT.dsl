/*
 * Intel ACPI Component Architecture
 * AML/ASL+ Disassembler version 20251212 (64-bit version)
 * Copyright (c) 2000 - 2025 Intel Corporation
 * 
 * Disassembling to symbolic ASL+ operators
 *
 * Disassembly of nc110-DSDT.aml
 *
 * Original Table Header:
 *     Signature        "DSDT"
 *     Length           0x00005F88 (24456)
 *     Revision         0x01 **** 32-bit table (V1), no 64-bit math support
 *     Checksum         0xBA
 *     OEM ID           "INTEL"
 *     OEM Table ID     "BEARG31A"
 *     OEM Revision     0x06040000 (100925440)
 *     Compiler ID      "MSFT"
 *     Compiler Version 0x03000001 (50331649)
 */
DefinitionBlock ("", "DSDT", 1, "INTEL", "BEARG31A", 0x06040000)
{
    External (_PR_.CPU0._PPC, UnknownObj)
    External (_SB_.VDRV, UnknownObj)
    External (LNKA, UnknownObj)
    External (LNKB, UnknownObj)
    External (LNKC, UnknownObj)
    External (LNKD, UnknownObj)
    External (LNKE, UnknownObj)
    External (LNKF, UnknownObj)
    External (LNKG, UnknownObj)
    External (LNKH, UnknownObj)
    External (PDC0, UnknownObj)
    External (PDC1, UnknownObj)
    External (PDC2, UnknownObj)
    External (PDC3, UnknownObj)

    OperationRegion (PRT0, SystemIO, 0x80, 0x04)
    Field (PRT0, DWordAcc, Lock, Preserve)
    {
        P80H,   32
    }

    OperationRegion (IO_T, SystemIO, 0x0800, 0x10)
    Field (IO_T, ByteAcc, NoLock, Preserve)
    {
        Offset (0x08), 
        TRP0,   8
    }

    OperationRegion (GNVS, SystemMemory, 0x3F5C0D7C, 0x0100)
    Field (GNVS, AnyAcc, Lock, Preserve)
    {
        OSYS,   16, 
        SMIF,   8, 
        PRM0,   8, 
        PRM1,   8, 
        SCIF,   8, 
        PRM2,   8, 
        PRM3,   8, 
        LCKF,   8, 
        PRM4,   8, 
        PRM5,   8, 
        P80D,   32, 
        LIDS,   8, 
        PWRS,   8, 
        DBGS,   8, 
        LINX,   8, 
        Offset (0x14), 
        ACTT,   8, 
        PSVT,   8, 
        TC1V,   8, 
        TC2V,   8, 
        TSPV,   8, 
        CRTT,   8, 
        DTSE,   8, 
        DTS1,   8, 
        DTS2,   8, 
        Offset (0x1E), 
        BNUM,   8, 
        B0SC,   8, 
        B1SC,   8, 
        B2SC,   8, 
        B0SS,   8, 
        B1SS,   8, 
        B2SS,   8, 
        Offset (0x28), 
        APIC,   8, 
        MPEN,   8, 
        Offset (0x2B), 
        PPCM,   8, 
        PCP0,   8, 
        PCP1,   8, 
        Offset (0x32), 
        NATP,   8, 
        CMAP,   8, 
        CMBP,   8, 
        LPTP,   8, 
        FDCP,   8, 
        CMCP,   8, 
        CIRP,   8, 
        Offset (0x3C), 
        IGDS,   8, 
        TLST,   8, 
        CADL,   8, 
        PADL,   8, 
        CSTE,   16, 
        NSTE,   16, 
        SSTE,   16, 
        NDID,   8, 
        DID1,   32, 
        DID2,   32, 
        DID3,   32, 
        DID4,   32, 
        DID5,   32, 
        Offset (0x67), 
        BLCS,   8, 
        BRTL,   8, 
        ALSE,   8, 
        ALAF,   8, 
        LLOW,   8, 
        LHIH,   8, 
        Offset (0x6E), 
        EMAE,   8, 
        EMAP,   16, 
        EMAL,   16, 
        Offset (0x74), 
        MEFE,   8, 
        Offset (0x78), 
        TPMP,   8, 
        TPME,   8, 
        Offset (0x82), 
        GTF0,   56, 
        GTF2,   56, 
        IDEM,   8, 
        VDRV,   8
    }

    OperationRegion (RCRB, SystemMemory, 0xFED1C000, 0x00004000)
    Field (RCRB, DWordAcc, Lock, Preserve)
    {
        Offset (0x1000), 
        Offset (0x3000), 
        Offset (0x3404), 
        HPAS,   2, 
            ,   5, 
        HPAE,   1, 
        Offset (0x3418), 
            ,   1, 
        PATD,   1, 
        SATD,   1, 
        SMBD,   1, 
        HDAD,   1, 
        Offset (0x341A), 
        RP1D,   1, 
        RP2D,   1, 
        RP3D,   1, 
        RP4D,   1, 
        RP5D,   1, 
        RP6D,   1
    }

    OperationRegion (PMIO, SystemIO, 0x1000, 0x80)
    Field (PMIO, ByteAcc, NoLock, Preserve)
    {
        Offset (0x42), 
            ,   1, 
        GPEC,   1
    }

    Scope (_GPE)
    {
        Method (_L01, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            L01C += 0x01
            P8XH (0x00, 0x01)
            P8XH (0x01, L01C)
            Sleep (0x64)
            If (((RP1D == 0x00) && \_SB.PCI0.EXP1.HPCS))
            {
                If (\_SB.PCI0.EXP1.PDC1)
                {
                    \_SB.PCI0.EXP1.PDC1 = 0x01
                    \_SB.PCI0.EXP1.HPCS = 0x01
                    Notify (\_SB.PCI0.EXP1, 0x00) // Bus Check
                }
                Else
                {
                    \_SB.PCI0.EXP1.HPCS = 0x01
                }
            }

            If (((RP2D == 0x00) && \_SB.PCI0.EXP2.HPCS))
            {
                If (\_SB.PCI0.EXP2.PDC2)
                {
                    \_SB.PCI0.EXP2.PDC2 = 0x01
                    \_SB.PCI0.EXP2.HPCS = 0x01
                    Notify (\_SB.PCI0.EXP2, 0x00) // Bus Check
                }
                Else
                {
                    \_SB.PCI0.EXP2.HPCS = 0x01
                }
            }

            If (((RP3D == 0x00) && \_SB.PCI0.EXP3.HPCS))
            {
                If (\_SB.PCI0.EXP3.PDC3)
                {
                    \_SB.PCI0.EXP3.PDC3 = 0x01
                    \_SB.PCI0.EXP3.HPCS = 0x01
                    Notify (\_SB.PCI0.EXP3, 0x00) // Bus Check
                }
                Else
                {
                    \_SB.PCI0.EXP3.HPCS = 0x01
                }
            }

            If (((RP4D == 0x00) && \_SB.PCI0.EXP4.HPCS))
            {
                If (\_SB.PCI0.EXP4.PDC4)
                {
                    \_SB.PCI0.EXP4.PDC4 = 0x01
                    \_SB.PCI0.EXP4.HPCS = 0x01
                    Notify (\_SB.PCI0.EXP4, 0x00) // Bus Check
                }
                Else
                {
                    \_SB.PCI0.EXP4.HPCS = 0x01
                }
            }
        }

        Method (_L02, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            GPEC = 0x00
        }

        Method (_L03, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.USB1, 0x02) // Device Wake
            Notify (\_SB.PWRB, 0x02) // Device Wake
        }

        Method (_L04, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.USB2, 0x02) // Device Wake
            Notify (\_SB.PWRB, 0x02) // Device Wake
        }

        Method (_L06, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            If (\_SB.PCI0.IGD0.GSSE)
            {
                \_SB.PCI0.IGD0.OPRN ()
            }
            Else
            {
                \_SB.PCI0.LPC0.SCIS = 0x01
            }
        }

        Method (_L09, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            If (\_SB.PCI0.EXP1.PSP1)
            {
                \_SB.PCI0.EXP1.PSP1 = 0x01
                \_SB.PCI0.EXP1.PMCS = 0x01
                Notify (\_SB.PCI0.EXP1, 0x02) // Device Wake
            }

            If (\_SB.PCI0.EXP2.PSP2)
            {
                \_SB.PCI0.EXP2.PSP2 = 0x01
                \_SB.PCI0.EXP2.PMCS = 0x01
                Notify (\_SB.PCI0.EXP2, 0x02) // Device Wake
            }

            If (\_SB.PCI0.EXP3.PSP3)
            {
                \_SB.PCI0.EXP3.PSP3 = 0x01
                \_SB.PCI0.EXP3.PMCS = 0x01
                Notify (\_SB.PCI0.EXP3, 0x02) // Device Wake
            }

            If (\_SB.PCI0.EXP4.PSP4)
            {
                \_SB.PCI0.EXP4.PSP4 = 0x01
                \_SB.PCI0.EXP4.PMCS = 0x01
                Notify (\_SB.PCI0.EXP4, 0x02) // Device Wake
            }
        }

        Method (_L0B, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.PCIB, 0x02) // Device Wake
        }

        Method (_L0C, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.USB3, 0x02) // Device Wake
        }

        Method (_L0D, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.EUSB, 0x02) // Device Wake
            Notify (\_SB.PWRB, 0x02) // Device Wake
        }

        Method (_L0E, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            Notify (\_SB.PCI0.USB4, 0x02) // Device Wake
            Notify (\_SB.PWRB, 0x02) // Device Wake
        }

        Method (_L1D, 0, NotSerialized)  // _Lxx: Level-Triggered GPE, xx=0x00-0xFF
        {
            TRAP (0x2B)
            PNOT ()
            Notify (\_SB.PWRB, 0x02) // Device Wake
        }
    }

    Scope (_PR)
    {
        Processor (CPU0, 0x00, 0x00001010, 0x06){}
        Processor (CPU1, 0x01, 0x00001010, 0x06){}
        Processor (CPU2, 0x02, 0x00001010, 0x06){}
        Processor (CPU3, 0x03, 0x00001010, 0x06){}
        Processor (CPU4, 0x04, 0x00001010, 0x06){}
        Processor (CPU5, 0x05, 0x00001010, 0x06){}
        Processor (CPU6, 0x06, 0x00001010, 0x06){}
        Processor (CPU7, 0x07, 0x00001010, 0x06){}
    }

    Name (\DSEN, 0x01)
    Name (\ECON, 0x00)
    Name (\GPIC, 0x00)
    Name (\CTYP, 0x00)
    Name (\L01C, 0x00)
    Name (\VFN0, 0x00)
    Scope (\_SB)
    {
        OperationRegion (TCG1, SystemMemory, 0x3F5C0C76, 0x00000007)
        Field (TCG1, AnyAcc, NoLock, Preserve)
        {
            PPRQ,   8, 
            PPLO,   8, 
            PPRP,   8, 
            PPOR,   8, 
            TPRS,   8, 
            TPMV,   8, 
            MOR,    8
        }

        Method (PHSR, 1, Serialized)
        {
            BCMD = Arg0
            DID = Zero
            SMIC = Zero
            If ((BCMD == Arg0)){}
            BCMD = Zero
            DID = Zero
            Return (0x00)
        }

        OperationRegion (SMI0, SystemIO, 0x0000FE00, 0x00000002)
        Field (SMI0, AnyAcc, NoLock, Preserve)
        {
            SMIC,   8
        }

        OperationRegion (SMI1, SystemMemory, 0x3F5C2EBD, 0x00000090)
        Field (SMI1, AnyAcc, NoLock, Preserve)
        {
            BCMD,   8, 
            DID,    32, 
            INFO,   1024
        }

        Field (SMI1, AnyAcc, NoLock, Preserve)
        {
            AccessAs (ByteAcc, 0x00), 
            Offset (0x05), 
            INF,    8
        }

        Name (PR00, Package (0x0B)
        {
            Package (0x04)
            {
                0x001FFFFF, 
                0x01, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x00, 
                LNKH, 
                0x00
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x01, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x02, 
                LNKC, 
                0x00
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x03, 
                LNKA, 
                0x00
            }, 

            Package (0x04)
            {
                0x001BFFFF, 
                0x00, 
                LNKG, 
                0x00
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x00, 
                LNKA, 
                0x00
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x01, 
                LNKB, 
                0x00
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x02, 
                LNKC, 
                0x00
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x03, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0x0002FFFF, 
                0x00, 
                LNKA, 
                0x00
            }
        })
        Name (AR00, Package (0x0B)
        {
            Package (0x04)
            {
                0x001FFFFF, 
                0x01, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x00, 
                0x00, 
                0x17
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x01, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x02, 
                0x00, 
                0x12
            }, 

            Package (0x04)
            {
                0x001DFFFF, 
                0x03, 
                0x00, 
                0x10
            }, 

            Package (0x04)
            {
                0x001BFFFF, 
                0x00, 
                0x00, 
                0x16
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x00, 
                0x00, 
                0x10
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x01, 
                0x00, 
                0x11
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x02, 
                0x00, 
                0x12
            }, 

            Package (0x04)
            {
                0x001CFFFF, 
                0x03, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0x0002FFFF, 
                0x00, 
                0x00, 
                0x10
            }
        })
        Name (PR04, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                LNKA, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                LNKB, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                LNKC, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                LNKD, 
                0x00
            }
        })
        Name (AR04, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                0x00, 
                0x10
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                0x00, 
                0x11
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                0x00, 
                0x12
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                0x00, 
                0x13
            }
        })
        Name (PR05, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                LNKB, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                LNKC, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                LNKA, 
                0x00
            }
        })
        Name (AR05, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                0x00, 
                0x11
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                0x00, 
                0x12
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                0x00, 
                0x10
            }
        })
        Name (PR06, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                LNKC, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                LNKA, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                LNKB, 
                0x00
            }
        })
        Name (AR06, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                0x00, 
                0x12
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                0x00, 
                0x10
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                0x00, 
                0x11
            }
        })
        Name (PR07, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                LNKD, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                LNKA, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                LNKB, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                LNKC, 
                0x00
            }
        })
        Name (AR07, Package (0x04)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                0x00, 
                0x13
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                0x00, 
                0x10
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                0x00, 
                0x11
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                0x00, 
                0x12
            }
        })
        Name (PR01, Package (0x05)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                LNKF, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                LNKG, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                LNKH, 
                0x00
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                LNKE, 
                0x00
            }, 

            Package (0x04)
            {
                0x0008FFFF, 
                0x00, 
                LNKE, 
                0x00
            }
        })
        Name (AR01, Package (0x05)
        {
            Package (0x04)
            {
                0xFFFF, 
                0x00, 
                0x00, 
                0x15
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x01, 
                0x00, 
                0x16
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x02, 
                0x00, 
                0x17
            }, 

            Package (0x04)
            {
                0xFFFF, 
                0x03, 
                0x00, 
                0x14
            }, 

            Package (0x04)
            {
                0x0008FFFF, 
                0x00, 
                0x00, 
                0x14
            }
        })
        Device (PCI0)
        {
            OperationRegion (MPCE, PCI_Config, 0x48, 0x04)
            Field (MPCE, DWordAcc, NoLock, Preserve)
            {
                PEXE,   1, 
                LENG,   2, 
                    ,   25, 
                EXBA,   4
            }

            Method (_INI, 0, NotSerialized)  // _INI: Initialize
            {
                If (DTSE)
                {
                    TRAP (0x47)
                }

                PEXE = 0x00
                \_SB.OSHT ()
                \_SB.SOST ()
                \_SB.SECS (0xA8)
                \_SB.SECS (0xAA)
                \_SB.SECB (0xB9, 0x00)
                P8XH (0x00, 0x0806)
            }

            Name (_HID, EisaId ("PNP0A08") /* PCI Express Bus */)  // _HID: Hardware ID
            Name (_CID, EisaId ("PNP0A03") /* PCI Bus */)  // _CID: Compatible ID
            Name (_ADR, 0x00)  // _ADR: Address
            Method (_OSC, 4, NotSerialized)  // _OSC: Operating System Capabilities
            {
                Local0 = Arg2
                Local1 = (Local0 * 0x04)
                Name (BUF1, Buffer (Local1){})
                BUF1 = Arg3
                Local1 = 0x00
                Local2 = 0x00
                While (Local0)
                {
                    Local2 = (Local1 * 0x04)
                    CreateDWordField (BUF1, Local2, CAPB)
                    If (Arg2)
                    {
                        (CAPB & 0xFFFFFFFC)
                        (CAPB | 0x00)
                    }
                    Else
                    {
                    }

                    Local1++
                    Local0--
                }

                Return (BUF1) /* \_SB_.PCI0._OSC.BUF1 */
            }

            OperationRegion (REGS, PCI_Config, 0x40, 0xC0)
            Field (REGS, ByteAcc, NoLock, Preserve)
            {
                Offset (0x50), 
                PAM0,   8, 
                PAM1,   8, 
                PAM2,   8, 
                PAM3,   8, 
                PAM4,   8, 
                PAM5,   8, 
                PAM6,   8, 
                    ,   7, 
                HEN,    1, 
                Offset (0x60), 
                TASM,   10, 
                Offset (0x62), 
                Offset (0x70), 
                Z000,   16
            }

            Name (RSRC, ResourceTemplate ()
            {
                WordBusNumber (ResourceProducer, MinFixed, MaxFixed, PosDecode,
                    0x0000,             // Granularity
                    0x0000,             // Range Minimum
                    0x003F,             // Range Maximum
                    0x0000,             // Translation Offset
                    0x0040,             // Length
                    0x00,, )
                IO (Decode16,
                    0x0CF8,             // Range Minimum
                    0x0CF8,             // Range Maximum
                    0x01,               // Alignment
                    0x08,               // Length
                    )
                DWordIO (ResourceProducer, MinFixed, MaxFixed, PosDecode, EntireRange,
                    0x00000000,         // Granularity
                    0x00000000,         // Range Minimum
                    0x00000CF7,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00000CF8,         // Length
                    0x00,, , TypeStatic, DenseTranslation)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000A0000,         // Range Minimum
                    0x000BFFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00020000,         // Length
                    0x00,, , AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000C0000,         // Range Minimum
                    0x000C3FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y01, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000C4000,         // Range Minimum
                    0x000C7FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y02, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000C8000,         // Range Minimum
                    0x000CBFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y03, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000CC000,         // Range Minimum
                    0x000CFFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y04, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000D0000,         // Range Minimum
                    0x000D3FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y05, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000D4000,         // Range Minimum
                    0x000D7FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y06, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000D8000,         // Range Minimum
                    0x000DBFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y07, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000DC000,         // Range Minimum
                    0x000DFFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y08, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000E0000,         // Range Minimum
                    0x000E3FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y09, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000E4000,         // Range Minimum
                    0x000E7FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y0A, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000E8000,         // Range Minimum
                    0x000EBFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y0B, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000EC000,         // Range Minimum
                    0x000EFFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00004000,         // Length
                    0x00,, _Y0C, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x000F0000,         // Range Minimum
                    0x000FFFFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00010000,         // Length
                    0x00,, _Y0D, AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x00000000,         // Range Minimum
                    0x00000000,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00000000,         // Length
                    0x00,, _Y00, AddressRangeMemory, TypeStatic)
                DWordIO (ResourceProducer, MinFixed, MaxFixed, PosDecode, EntireRange,
                    0x00000000,         // Granularity
                    0x00000D00,         // Range Minimum
                    0x0000FDFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x0000F100,         // Length
                    0x00,, , TypeStatic, DenseTranslation)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0x00000000,         // Range Minimum
                    0x00000000,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00000000,         // Length
                    0x00,, , AddressRangeMemory, TypeStatic)
                DWordMemory (ResourceProducer, PosDecode, MinFixed, MaxFixed, Cacheable, ReadWrite,
                    0x00000000,         // Granularity
                    0xFED40000,         // Range Minimum
                    0xFED44FFF,         // Range Maximum
                    0x00000000,         // Translation Offset
                    0x00000000,         // Length
                    ,, _Y0E, AddressRangeMemory, TypeStatic)
            })
            Method (_CRS, 0, Serialized)  // _CRS: Current Resource Settings
            {
                Local1 = Zero
                CreateDWordField (RSRC, \_SB.PCI0._Y00._MIN, BTMN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y00._MAX, BTMX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y00._LEN, BTLN)  // _LEN: Length
                BTMN = ((Z000 & 0xFFF0) << 0x10)
                BTLN = (0xF8000000 - BTMN) /* \_SB_.PCI0._CRS.BTMN */
                BTMX = ((BTMN + BTLN) - 0x01)
                CreateBitField (RSRC, \_SB.PCI0._Y01._RW, C0RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y01._MIN, C0MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y01._MAX, C0MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y01._LEN, C0LN)  // _LEN: Length
                C0RW = One
                If (((PAM1 & 0x03) == 0x01))
                {
                    C0RW = Zero
                }

                C0LN = Zero
                If (!(PAM1 & 0x03))
                {
                    C0LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y02._RW, C4RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y02._MIN, C4MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y02._MAX, C4MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y02._LEN, C4LN)  // _LEN: Length
                C4RW = One
                If (((PAM1 & 0x30) == 0x10))
                {
                    C4RW = Zero
                }

                C4LN = Zero
                If (!(PAM1 & 0x30))
                {
                    C4LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y03._RW, C8RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y03._MIN, C8MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y03._MAX, C8MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y03._LEN, C8LN)  // _LEN: Length
                C8RW = One
                If (((PAM2 & 0x03) == 0x01))
                {
                    C8RW = Zero
                }

                C8LN = Zero
                If (!(PAM2 & 0x03))
                {
                    C8LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y04._RW, CCRW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y04._MIN, CCMN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y04._MAX, CCMX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y04._LEN, CCLN)  // _LEN: Length
                CCRW = One
                If (((PAM2 & 0x30) == 0x10))
                {
                    CCRW = Zero
                }

                CCLN = Zero
                If (!(PAM2 & 0x30))
                {
                    CCLN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y05._RW, D0RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y05._MIN, D0MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y05._MAX, D0MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y05._LEN, D0LN)  // _LEN: Length
                D0RW = One
                If (((PAM3 & 0x03) == 0x01))
                {
                    D0RW = Zero
                }

                D0LN = Zero
                If (!(PAM3 & 0x03))
                {
                    D0LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y06._RW, D4RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y06._MIN, D4MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y06._MAX, D4MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y06._LEN, D4LN)  // _LEN: Length
                D4RW = One
                If (((PAM3 & 0x30) == 0x10))
                {
                    D4RW = Zero
                }

                D4LN = Zero
                If (!(PAM3 & 0x30))
                {
                    D4LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y07._RW, D8RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y07._MIN, D8MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y07._MAX, D8MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y07._LEN, D8LN)  // _LEN: Length
                D8RW = One
                If (((PAM4 & 0x03) == 0x01))
                {
                    D8RW = Zero
                }

                D8LN = Zero
                If (!(PAM4 & 0x03))
                {
                    D8LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y08._RW, DCRW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y08._MIN, DCMN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y08._MAX, DCMX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y08._LEN, DCLN)  // _LEN: Length
                DCRW = One
                If (((PAM4 & 0x30) == 0x10))
                {
                    DCRW = Zero
                }

                DCLN = Zero
                If (!(PAM4 & 0x30))
                {
                    DCLN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y09._RW, E0RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y09._MIN, E0MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y09._MAX, E0MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y09._LEN, E0LN)  // _LEN: Length
                E0RW = One
                If (((PAM5 & 0x03) == 0x01))
                {
                    E0RW = Zero
                }

                E0LN = Zero
                If (!(PAM5 & 0x03))
                {
                    E0LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y0A._RW, E4RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y0A._MIN, E4MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0A._MAX, E4MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0A._LEN, E4LN)  // _LEN: Length
                E4RW = One
                If (((PAM5 & 0x30) == 0x10))
                {
                    E4RW = Zero
                }

                E4LN = Zero
                If (!(PAM5 & 0x30))
                {
                    E4LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y0B._RW, E8RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y0B._MIN, E8MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0B._MAX, E8MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0B._LEN, E8LN)  // _LEN: Length
                E8RW = One
                If (((PAM6 & 0x03) == 0x01))
                {
                    E8RW = Zero
                }

                E8LN = Zero
                If (!(PAM6 & 0x03))
                {
                    E8LN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y0C._RW, ECRW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y0C._MIN, ECMN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0C._MAX, ECMX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0C._LEN, ECLN)  // _LEN: Length
                ECRW = One
                If (((PAM6 & 0x30) == 0x10))
                {
                    ECRW = Zero
                }

                ECLN = Zero
                If (!(PAM6 & 0x30))
                {
                    ECLN = 0x4000
                }

                CreateBitField (RSRC, \_SB.PCI0._Y0D._RW, F0RW)  // _RW_: Read-Write Status
                CreateDWordField (RSRC, \_SB.PCI0._Y0D._MIN, F0MN)  // _MIN: Minimum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0D._MAX, F0MX)  // _MAX: Maximum Base Address
                CreateDWordField (RSRC, \_SB.PCI0._Y0D._LEN, F0LN)  // _LEN: Length
                F0RW = One
                If (((PAM0 & 0x30) == 0x10))
                {
                    F0RW = Zero
                }

                F0LN = Zero
                If (!(PAM0 & 0x30))
                {
                    F0LN = 0x00010000
                }

                If (TPMP)
                {
                    CreateDWordField (RSRC, \_SB.PCI0._Y0E._LEN, TPML)  // _LEN: Length
                    TPML = 0x5000
                }

                Return (RSRC) /* \_SB_.PCI0.RSRC */
            }

            Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
            {
                Return (0x02)
            }

            Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
            {
                Return (0x02)
            }

            Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
            {
                If (PICF)
                {
                    Return (AR00) /* \_SB_.AR00 */
                }

                Return (PR00) /* \_SB_.PR00 */
            }

            Device (IGD0)
            {
                Name (_ADR, 0x00020000)  // _ADR: Address
                OperationRegion (IGDP, PCI_Config, 0x40, 0xC0)
                Field (IGDP, AnyAcc, NoLock, Preserve)
                {
                    Offset (0x12), 
                        ,   1, 
                    GIVD,   1, 
                        ,   2, 
                    GUMA,   3, 
                    Offset (0x14), 
                        ,   4, 
                    GMFN,   1, 
                    Offset (0x18), 
                    SSRW,   32, 
                    Offset (0xA4), 
                    ASLE,   8, 
                    Offset (0xA8), 
                    GSSE,   1, 
                    GSSB,   14, 
                    GSES,   1, 
                    Offset (0xB0), 
                    Offset (0xB1), 
                    CDVL,   5, 
                    Offset (0xB2), 
                    Offset (0xBC), 
                    ASLS,   32
                }

                Method (_STA, 0, NotSerialized)  // _STA: Status
                {
                    If ((ASLS == 0xFEF00000))
                    {
                        Return (0x00)
                    }
                    Else
                    {
                        Return (0x0F)
                    }
                }

                OperationRegion (IGDM, SystemMemory, 0x3F5C0E7C, 0x00002040)
                Field (IGDM, AnyAcc, NoLock, Preserve)
                {
                    SIGN,   128, 
                    SIZE,   32, 
                    OVER,   32, 
                    SVER,   256, 
                    VVER,   128, 
                    GVER,   128, 
                    MBOX,   32, 
                    Offset (0x100), 
                    DRDY,   32, 
                    CSTS,   32, 
                    CEVT,   32, 
                    Offset (0x120), 
                    DIDL,   256, 
                    CPDL,   256, 
                    CADL,   256, 
                    NADL,   256, 
                    ASLP,   32, 
                    TIDX,   32, 
                    CHPD,   32, 
                    CLID,   32, 
                    CDCK,   32, 
                    SXSW,   32, 
                    EVTS,   32, 
                    CNOT,   32, 
                    NRDY,   32, 
                    Offset (0x200), 
                    SCIE,   1, 
                    GEFC,   4, 
                    GXFC,   3, 
                    GESF,   8, 
                    Offset (0x204), 
                    PARM,   32, 
                    DSLP,   32, 
                    Offset (0x300), 
                    ARDY,   32, 
                    ASLC,   32, 
                    TCHE,   32, 
                    ALSI,   32, 
                    BCLP,   32, 
                    PFIT,   32, 
                    Offset (0x400), 
                    GVD1,   57344, 
                    IBTT,   8, 
                    IPAT,   8, 
                    ITVF,   8, 
                    ITVM,   8, 
                    IPSC,   8, 
                    IBLC,   8, 
                    IBIA,   8, 
                    ISSC,   8, 
                    I409,   8, 
                    I509,   8, 
                    I609,   8, 
                    I709,   8, 
                    IDMM,   8, 
                    IDMS,   8, 
                    IF1E,   8, 
                    GSMI,   8, 
                    HVCO,   8, 
                    LIDS,   8, 
                    CGCS,   16
                }

                Name (DBTB, Package (0x11)
                {
                    0x00, 
                    0x01, 
                    0x02, 
                    0x03, 
                    0x04, 
                    0x05, 
                    0x06, 
                    0x07, 
                    0x08, 
                    0x09, 
                    0x0A, 
                    0x10, 
                    0x11, 
                    0x12, 
                    0x13, 
                    0x14, 
                    0xFF
                })
                Name (SUCC, 0x01)
                Name (NVLD, 0x02)
                Name (CRIT, 0x04)
                Name (NCRT, 0x06)
                Method (GBDA, 0, Serialized)
                {
                    If ((GESF == 0x00))
                    {
                        PARM = 0x00
                        PARM |= (0x01 << 0x00)
                        PARM |= (0x01 << 0x03)
                        PARM |= (0x01 << 0x06)
                        GESF = Zero
                        Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                    }

                    If ((GESF == 0x01))
                    {
                        PARM = 0x00
                        GESF = Zero
                        Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                    }

                    If ((GESF == 0x04))
                    {
                        Name (LOOP, 0x00)
                        PARM &= 0xFFFFFF00
                        PARM |= DerefOf (DBTB [IBTT]) /* \_SB_.PCI0.IGD0.PARM */
                        GESF = Zero
                        Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                    }

                    If ((GESF == 0x05)){}
                    If ((GESF == 0x07))
                    {
                        PARM = GIVD /* \_SB_.PCI0.IGD0.GIVD */
                        PARM ^= 0x01
                        PARM |= (GMFN << 0x01)
                        PARM |= (0x02 << 0x0B)
                        If ((IDMM == 0x00))
                        {
                            PARM |= (IDMS << 0x0D)
                        }

                        If (((IDMM == 0x01) || (IDMM == 0x03)))
                        {
                            PARM |= (IDMS << 0x11)
                        }

                        PARM |= (CGCS << 0x15)
                        GESF = 0x01
                        Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                    }

                    If ((GESF == 0x0A)){}
                    GESF = Zero
                    Return (CRIT) /* \_SB_.PCI0.IGD0.CRIT */
                }

                Method (SBCB, 0, Serialized)
                {
                    If ((GESF == 0x00))
                    {
                        PARM = 0x00
                        GESF = Zero
                        Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                    }

                    GESF = Zero
                    Return (SUCC) /* \_SB_.PCI0.IGD0.SUCC */
                }

                Method (OPRN, 0, Serialized)
                {
                    If ((GEFC == 0x04))
                    {
                        GXFC = GBDA ()
                    }

                    If ((GEFC == 0x06))
                    {
                        GXFC = SBCB ()
                    }

                    SSRW = PARM /* \_SB_.PCI0.IGD0.PARM */
                    GEFC = 0x00
                    \_SB.PCI0.LPC0.SCIS = 0x01
                    GSSE = 0x00
                    SCIE = 0x00
                    Return (Zero)
                }

                Method (_DOS, 1, NotSerialized)  // _DOS: Disable Output Switching
                {
                    DSEN = (Arg0 & 0x03)
                }

                Method (_DOD, 0, NotSerialized)  // _DOD: Display Output Devices
                {
                    If ((NDID == 0x01))
                    {
                        Name (TMP1, Package (0x01)
                        {
                            0xFFFFFFFF
                        })
                        TMP1 [0x00] = (0x00010000 | DID1)
                        Return (TMP1) /* \_SB_.PCI0.IGD0._DOD.TMP1 */
                    }

                    If ((NDID == 0x02))
                    {
                        Name (TMP2, Package (0x02)
                        {
                            0xFFFFFFFF, 
                            0xFFFFFFFF
                        })
                        TMP2 [0x00] = (0x00010000 | DID1)
                        TMP2 [0x01] = (0x00010000 | DID2)
                        Return (TMP2) /* \_SB_.PCI0.IGD0._DOD.TMP2 */
                    }

                    If ((NDID == 0x03))
                    {
                        Name (TMP3, Package (0x03)
                        {
                            0xFFFFFFFF, 
                            0xFFFFFFFF, 
                            0xFFFFFFFF
                        })
                        TMP3 [0x00] = (0x00010000 | DID1)
                        TMP3 [0x01] = (0x00010000 | DID2)
                        TMP3 [0x02] = (0x00010000 | DID3)
                        Return (TMP3) /* \_SB_.PCI0.IGD0._DOD.TMP3 */
                    }

                    If ((NDID == 0x04))
                    {
                        Name (TMP4, Package (0x04)
                        {
                            0xFFFFFFFF, 
                            0xFFFFFFFF, 
                            0xFFFFFFFF, 
                            0xFFFFFFFF
                        })
                        TMP4 [0x00] = (0x00010000 | DID1)
                        TMP4 [0x01] = (0x00010000 | DID2)
                        TMP4 [0x02] = (0x00010000 | DID3)
                        TMP4 [0x03] = (0x00010000 | DID4)
                        Return (TMP4) /* \_SB_.PCI0.IGD0._DOD.TMP4 */
                    }

                    Name (TMP5, Package (0x05)
                    {
                        0xFFFFFFFF, 
                        0xFFFFFFFF, 
                        0xFFFFFFFF, 
                        0xFFFFFFFF, 
                        0xFFFFFFFF
                    })
                    TMP5 [0x00] = (0x00010000 | DID1)
                    TMP5 [0x01] = (0x00010000 | DID2)
                    TMP5 [0x02] = (0x00010000 | DID3)
                    TMP5 [0x03] = (0x00010000 | DID4)
                    TMP5 [0x04] = (0x00010000 | DID5)
                    Return (TMP5) /* \_SB_.PCI0.IGD0._DOD.TMP5 */
                }

                Device (DD01)
                {
                    Method (_ADR, 0, Serialized)  // _ADR: Address
                    {
                        Return ((0xFFFF & DID1))
                    }

                    Method (_DCS, 0, NotSerialized)  // _DCS: Display Current Status
                    {
                        TRAP (0x01)
                        If ((CSTE & 0x01))
                        {
                            Return (0x1F)
                        }

                        Return (0x1D)
                    }

                    Method (_DGS, 0, NotSerialized)  // _DGS: Display Graphics State
                    {
                        If ((NSTE & 0x01))
                        {
                            Return (0x01)
                        }

                        Return (0x00)
                    }

                    Method (_DSS, 1, NotSerialized)  // _DSS: Device Set State
                    {
                        If (((Arg0 & 0xC0000000) == 0xC0000000))
                        {
                            CSTE = NSTE /* \_SB_.NSTE */
                        }
                    }
                }

                Device (DD02)
                {
                    Method (_ADR, 0, Serialized)  // _ADR: Address
                    {
                        Return ((0xFFFF & DID2))
                    }

                    Method (_DCS, 0, NotSerialized)  // _DCS: Display Current Status
                    {
                        TRAP (0x01)
                        If ((CSTE & 0x02))
                        {
                            Return (0x1F)
                        }

                        Return (0x1D)
                    }

                    Method (_DGS, 0, NotSerialized)  // _DGS: Display Graphics State
                    {
                        If ((NSTE & 0x02))
                        {
                            Return (0x01)
                        }

                        Return (0x00)
                    }

                    Method (_DSS, 1, NotSerialized)  // _DSS: Device Set State
                    {
                        If (((Arg0 & 0xC0000000) == 0xC0000000))
                        {
                            CSTE = NSTE /* \_SB_.NSTE */
                        }
                    }
                }

                Device (DD03)
                {
                    Method (_ADR, 0, Serialized)  // _ADR: Address
                    {
                        Return ((0xFFFF & DID3))
                    }

                    Method (_DCS, 0, NotSerialized)  // _DCS: Display Current Status
                    {
                        TRAP (0x01)
                        If ((CSTE & 0x04))
                        {
                            Return (0x1F)
                        }

                        Return (0x1D)
                    }

                    Method (_DGS, 0, NotSerialized)  // _DGS: Display Graphics State
                    {
                        If ((NSTE & 0x04))
                        {
                            Return (0x01)
                        }

                        Return (0x00)
                    }

                    Method (_DSS, 1, NotSerialized)  // _DSS: Device Set State
                    {
                        If (((Arg0 & 0xC0000000) == 0xC0000000))
                        {
                            CSTE = NSTE /* \_SB_.NSTE */
                        }
                    }
                }

                Device (DD04)
                {
                    Method (_ADR, 0, Serialized)  // _ADR: Address
                    {
                        Return ((0xFFFF & DID4))
                    }

                    Method (_DCS, 0, NotSerialized)  // _DCS: Display Current Status
                    {
                        TRAP (0x01)
                        If ((CSTE & 0x08))
                        {
                            Return (0x1F)
                        }

                        Return (0x1D)
                    }

                    Method (_DGS, 0, NotSerialized)  // _DGS: Display Graphics State
                    {
                        If ((NSTE & 0x08))
                        {
                            Return (0x01)
                        }

                        Return (0x00)
                    }

                    Method (_DSS, 1, NotSerialized)  // _DSS: Device Set State
                    {
                        If (((Arg0 & 0xC0000000) == 0xC0000000))
                        {
                            CSTE = NSTE /* \_SB_.NSTE */
                        }
                    }

                    Method (_BCL, 0, NotSerialized)  // _BCL: Brightness Control Levels
                    {
                        VDRV |= 0x01
                        Return (Package (0x08)
                        {
                            0x64, 
                            0x05, 
                            0x0C, 
                            0x14, 
                            0x1E, 
                            0x2D, 
                            0x3C, 
                            0x50
                        })
                    }

                    Method (_BCM, 1, NotSerialized)  // _BCM: Brightness Control Method
                    {
                        Divide (Arg0, 0x0A, Local0, Local1)
                        If ((Local0 == 0x00))
                        {
                            BRTW (Arg0)
                        }
                    }

                    Method (_BQC, 0, NotSerialized)  // _BQC: Brightness Query Current
                    {
                        Divide (BRTL, 0x0A, Local0, Local1)
                        If ((Local0 == 0x00))
                        {
                            Return (BRTL) /* \_SB_.BRTL */
                        }
                    }
                }

                Device (DD05)
                {
                    Method (_ADR, 0, Serialized)  // _ADR: Address
                    {
                        Return ((0xFFFF & DID5))
                    }

                    Method (_DCS, 0, NotSerialized)  // _DCS: Display Current Status
                    {
                        TRAP (0x01)
                        If ((CSTE & 0x10))
                        {
                            Return (0x1F)
                        }

                        Return (0x1D)
                    }

                    Method (_DGS, 0, NotSerialized)  // _DGS: Display Graphics State
                    {
                        If ((NSTE & 0x10))
                        {
                            Return (0x01)
                        }

                        Return (0x00)
                    }

                    Method (_DSS, 1, NotSerialized)  // _DSS: Device Set State
                    {
                        If (((Arg0 & 0xC0000000) == 0xC0000000))
                        {
                            CSTE = NSTE /* \_SB_.NSTE */
                        }
                    }
                }

                Method (BRTN, 1, Serialized)
                {
                    If (((DID1 & 0x0F00) == 0x0400))
                    {
                        Notify (\_SB.PCI0.IGD0.DD01, Arg0)
                    }

                    If (((DID2 & 0x0F00) == 0x0400))
                    {
                        Notify (\_SB.PCI0.IGD0.DD02, Arg0)
                    }

                    If (((DID3 & 0x0F00) == 0x0400))
                    {
                        Notify (\_SB.PCI0.IGD0.DD03, Arg0)
                    }

                    If (((DID4 & 0x0F00) == 0x0400))
                    {
                        Notify (\_SB.PCI0.IGD0.DD04, Arg0)
                    }

                    If (((DID5 & 0x0F00) == 0x0400))
                    {
                        Notify (\_SB.PCI0.IGD0.DD05, Arg0)
                    }
                }
            }

            Device (EXP1)
            {
                Name (_ADR, 0x001C0000)  // _ADR: Address
                OperationRegion (P1CS, PCI_Config, 0x40, 0x0100)
                Field (P1CS, AnyAcc, NoLock, WriteAsZeros)
                {
                    Offset (0x1A), 
                    ABP1,   1, 
                        ,   2, 
                    PDC1,   1, 
                        ,   2, 
                    PDS1,   1, 
                    Offset (0x20), 
                    Offset (0x22), 
                    PSP1,   1, 
                    Offset (0x9C), 
                        ,   30, 
                    HPCS,   1, 
                    PMCS,   1
                }

                Device (PXS1)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    OperationRegion (X1CS, PCI_Config, 0x00, 0x0100)
                    Field (X1CS, AnyAcc, NoLock, WriteAsZeros)
                    {
                        X1DV,   32
                    }

                    Method (_RMV, 0, NotSerialized)  // _RMV: Removal Status
                    {
                        Return (0x00)
                    }
                }

                Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                {
                    0x09, 
                    0x04
                })
                Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
                {
                    If (PICF)
                    {
                        Return (AR04) /* \_SB_.AR04 */
                    }

                    Return (PR04) /* \_SB_.PR04 */
                }
            }

            Device (EXP2)
            {
                Name (_ADR, 0x001C0001)  // _ADR: Address
                OperationRegion (P2CS, PCI_Config, 0x40, 0x0100)
                Field (P2CS, AnyAcc, NoLock, WriteAsZeros)
                {
                    Offset (0x1A), 
                    ABP2,   1, 
                        ,   2, 
                    PDC2,   1, 
                        ,   2, 
                    PDS2,   1, 
                    Offset (0x20), 
                    Offset (0x22), 
                    PSP2,   1, 
                    Offset (0x9C), 
                        ,   30, 
                    HPCS,   1, 
                    PMCS,   1
                }

                Device (PXS2)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    OperationRegion (X2CS, PCI_Config, 0x00, 0x0100)
                    Field (X2CS, AnyAcc, NoLock, WriteAsZeros)
                    {
                        X2DV,   32
                    }

                    Method (_RMV, 0, NotSerialized)  // _RMV: Removal Status
                    {
                        Return (0x00)
                    }
                }

                Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                {
                    0x09, 
                    0x04
                })
                Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
                {
                    If (PICF)
                    {
                        Return (AR05) /* \_SB_.AR05 */
                    }

                    Return (PR05) /* \_SB_.PR05 */
                }
            }

            Device (EXP3)
            {
                Name (_ADR, 0x001C0002)  // _ADR: Address
                OperationRegion (P3CS, PCI_Config, 0x40, 0x0100)
                Field (P3CS, AnyAcc, NoLock, WriteAsZeros)
                {
                    Offset (0x1A), 
                    ABP3,   1, 
                        ,   2, 
                    PDC3,   1, 
                        ,   2, 
                    PDS3,   1, 
                    Offset (0x20), 
                    Offset (0x22), 
                    PSP3,   1, 
                    Offset (0x9C), 
                        ,   30, 
                    HPCS,   1, 
                    PMCS,   1
                }

                Device (PXS3)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    OperationRegion (X3CS, PCI_Config, 0x00, 0x0100)
                    Field (X3CS, AnyAcc, NoLock, WriteAsZeros)
                    {
                        X3DV,   32
                    }

                    Method (_RMV, 0, NotSerialized)  // _RMV: Removal Status
                    {
                        Return (0x00)
                    }
                }

                Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                {
                    0x09, 
                    0x04
                })
                Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
                {
                    If (PICF)
                    {
                        Return (AR06) /* \_SB_.AR06 */
                    }

                    Return (PR06) /* \_SB_.PR06 */
                }
            }

            Device (EXP4)
            {
                Name (_ADR, 0x001C0003)  // _ADR: Address
                OperationRegion (P4CS, PCI_Config, 0x40, 0x0100)
                Field (P4CS, AnyAcc, NoLock, WriteAsZeros)
                {
                    Offset (0x1A), 
                    ABP4,   1, 
                        ,   2, 
                    PDC4,   1, 
                        ,   2, 
                    PDS4,   1, 
                    Offset (0x20), 
                    Offset (0x22), 
                    PSP4,   1, 
                    Offset (0x9C), 
                        ,   30, 
                    HPCS,   1, 
                    PMCS,   1
                }

                Device (PXS4)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    OperationRegion (X4CS, PCI_Config, 0x00, 0x0100)
                    Field (X4CS, AnyAcc, NoLock, WriteAsZeros)
                    {
                        X4DV,   32
                    }

                    Method (_RMV, 0, NotSerialized)  // _RMV: Removal Status
                    {
                        Return (0x00)
                    }
                }

                Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                {
                    0x09, 
                    0x04
                })
                Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
                {
                    If (PICF)
                    {
                        Return (AR07) /* \_SB_.AR07 */
                    }

                    Return (PR07) /* \_SB_.PR07 */
                }
            }

            Device (PCIB)
            {
                Name (_ADR, 0x001E0000)  // _ADR: Address
                Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                {
                    0x0B, 
                    0x03
                })
                Method (_PRT, 0, NotSerialized)  // _PRT: PCI Routing Table
                {
                    If (PICF)
                    {
                        Return (AR01) /* \_SB_.AR01 */
                    }

                    Return (PR01) /* \_SB_.PR01 */
                }
            }

            Device (LPC0)
            {
                Name (_ADR, 0x001F0000)  // _ADR: Address
                Name (DVEN, 0x00)
                OperationRegion (TCOI, SystemIO, 0x1060, 0x08)
                Field (TCOI, WordAcc, NoLock, Preserve)
                {
                    Offset (0x04), 
                        ,   9, 
                    SCIS,   1, 
                    Offset (0x06)
                }

                Device (H_EC)
                {
                    Name (_HID, EisaId ("PNP0C09") /* Embedded Controller Device */)  // _HID: Hardware ID
                    Name (_UID, 0x01)  // _UID: Unique ID
                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        Name (BFFR, ResourceTemplate ()
                        {
                            IO (Decode16,
                                0x0062,             // Range Minimum
                                0x0062,             // Range Maximum
                                0x00,               // Alignment
                                0x01,               // Length
                                )
                            IO (Decode16,
                                0x0066,             // Range Minimum
                                0x0066,             // Range Maximum
                                0x00,               // Alignment
                                0x01,               // Length
                                )
                        })
                        Return (BFFR) /* \_SB_.PCI0.LPC0.H_EC._CRS.BFFR */
                    }

                    OperationRegion (ECR, EmbeddedControl, 0x00, 0xFF)
                    Field (ECR, ByteAcc, Lock, Preserve)
                    {
                        Offset (0x18), 
                        SPTR,   8, 
                        SSTS,   8, 
                        SADR,   8, 
                        SCMD,   8, 
                        SBFR,   256, 
                        SCNT,   8, 
                        Offset (0x80), 
                        B1EX,   1, 
                            ,   1, 
                        ACEX,   1, 
                        Offset (0x81), 
                        SWBE,   1, 
                        DCBE,   1, 
                        Offset (0x82), 
                        WLST,   1, 
                        Offset (0x83), 
                        LIDS,   1, 
                        Offset (0x84), 
                        B1ST,   8, 
                        Offset (0x86), 
                        BRIT,   8, 
                        Offset (0xA0), 
                        B1RP,   16, 
                        B1RA,   16, 
                        B1PR,   16, 
                        B1VO,   16, 
                        Offset (0xB0), 
                        B1DA,   16, 
                        B1DF,   16, 
                        B1DV,   16, 
                        B1DL,   16, 
                        Offset (0xC0), 
                        CTMP,   8, 
                        Offset (0xC7), 
                        TIST,   8, 
                        Offset (0xD0), 
                        B1TI,   16, 
                        B1SE,   16, 
                        B1CR,   16, 
                        B1TM,   16
                    }

                    Device (BAT1)
                    {
                        Name (_HID, EisaId ("PNP0C0A") /* Control Method Battery */)  // _HID: Hardware ID
                        Name (_UID, 0x01)  // _UID: Unique ID
                        Name (BATI, Package (0x0D)
                        {
                            0x01, 
                            0xFFFFFFFF, 
                            0xFFFFFFFF, 
                            0x01, 
                            0xFFFFFFFF, 
                            0x03, 
                            0x0A, 
                            0x01, 
                            0x01, 
                            "Unknown", 
                            "Unknown", 
                            "Unknown", 
                            "Unknown"
                        })
                        Method (_BIF, 0, NotSerialized)  // _BIF: Battery Information
                        {
                            If ((\ECON == 0x00))
                            {
                                Local0 = \_SB.SECW (0x82, 0xB0, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x01] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x01] = Local0
                                }

                                Local0 = \_SB.SECW (0x82, 0xB2, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x02] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x02] = Local0
                                }

                                Local0 = \_SB.SECW (0x82, 0xB4, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x04] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x04] = Local0
                                }

                                BATI [0x05] = 0x00
                                Local0 = \_SB.SECW (0x82, 0xB6, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x06] = Zero
                                }
                                Else
                                {
                                    BATI [0x06] = Local0
                                }
                            }
                            Else
                            {
                                Local0 = \_SB.PCI0.LPC0.H_EC.B1DA
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x01] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x01] = Local0
                                }

                                Local0 = \_SB.PCI0.LPC0.H_EC.B1DF
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x02] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x02] = Local0
                                }

                                Local0 = \_SB.PCI0.LPC0.H_EC.B1DV
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x04] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x04] = Local0
                                }

                                BATI [0x05] = 0x00
                                Local0 = \_SB.PCI0.LPC0.H_EC.B1DL
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    BATI [0x06] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    BATI [0x06] = Local0
                                }
                            }

                            BATI [0x09] = ""
                            BATI [0x0A] = ""
                            BATI [0x0B] = "LION"
                            BATI [0x0C] = "SAMSUNG Electronics"
                            Return (BATI) /* \_SB_.PCI0.LPC0.H_EC.BAT1.BATI */
                        }

                        Name (STAT, Package (0x04)
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                        Method (_BST, 0, NotSerialized)  // _BST: Battery Status
                        {
                            If ((\ECON == 0x00))
                            {
                                Local0 = \_SB.SECB (0x81, 0x84)
                                If (((Local0 != 0x00) && (Local0 != 0x05)))
                                {
                                    If ((\PWRS == 0x01))
                                    {
                                        Local0 = 0x02
                                    }
                                    Else
                                    {
                                        Local0 = 0x01
                                    }
                                }

                                STAT [0x00] = Local0
                                Local0 = \_SB.SECW (0x82, 0xA4, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x01] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    If ((Local0 >= 0x8000))
                                    {
                                        Local0 ^= 0xFFFF
                                        Local0++
                                    }

                                    STAT [0x01] = Local0
                                }

                                Local0 = \_SB.SECW (0x82, 0xA2, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x02] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    STAT [0x02] = Local0
                                }

                                Local0 = \_SB.SECW (0x82, 0xA6, 0x00)
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x03] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    STAT [0x03] = Local0
                                }
                            }
                            Else
                            {
                                Local0 = \_SB.PCI0.LPC0.H_EC.B1ST
                                If (((Local0 != 0x00) && (Local0 != 0x05)))
                                {
                                    If ((\PWRS == 0x01))
                                    {
                                        Local0 = 0x02
                                    }
                                    Else
                                    {
                                        Local0 = 0x01
                                    }
                                }

                                STAT [0x00] = Local0
                                Local0 = \_SB.PCI0.LPC0.H_EC.B1PR
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x01] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    If ((Local0 >= 0x8000))
                                    {
                                        Local0 ^= 0xFFFF
                                        Local0++
                                    }

                                    STAT [0x01] = Local0
                                }

                                Local0 = \_SB.PCI0.LPC0.H_EC.B1RA
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x02] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    STAT [0x02] = Local0
                                }

                                Local0 = \_SB.PCI0.LPC0.H_EC.B1VO
                                Local1 = (Local0 << 0x08)
                                Local1 &= 0xFF00
                                Local0 >>= 0x08
                                Local0 |= Local1
                                If ((Local0 == 0xFFFF))
                                {
                                    STAT [0x03] = 0xFFFFFFFF
                                }
                                Else
                                {
                                    STAT [0x03] = Local0
                                }
                            }

                            Return (STAT) /* \_SB_.PCI0.LPC0.H_EC.BAT1.STAT */
                        }

                        Method (_STA, 0, NotSerialized)  // _STA: Status
                        {
                            If ((\ECON == 0x00))
                            {
                                If ((\_SB.SECB (0x85, 0x00) == 0x01))
                                {
                                    Local0 = 0x1F
                                }
                                Else
                                {
                                    Local0 = 0x0F
                                }
                            }
                            ElseIf ((\_SB.PCI0.LPC0.H_EC.B1EX == 0x01))
                            {
                                Local0 = 0x1F
                            }
                            Else
                            {
                                Local0 = 0x0F
                            }

                            Return (Local0)
                        }

                        Method (_PCL, 0, NotSerialized)  // _PCL: Power Consumer List
                        {
                            Return (\_SB)
                        }
                    }

                    Method (_REG, 2, NotSerialized)  // _REG: Region Availability
                    {
                        If (((Arg0 == 0x03) && (Arg1 == 0x01)))
                        {
                            ECON = 0x01
                            \PWRS = \_SB.PCI0.LPC0.H_EC.ACEX
                            If ((LIDS != \LIDS))
                            {
                                \LIDS = 0x01
                                LSDS (LIDS)
                                Notify (\_SB.LID0, 0x80) // Status Change
                            }
                        }
                    }

                    Name (_GPE, 0x11)  // _GPE: General Purpose Events
                    Method (_Q51, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        P8XH (0x00, 0x30)
                        Notify (\_SB.ADP1, 0x80) // Status Change
                        PWRS = 0x01
                        TRAP (0x2B)
                        PNOT ()
                        If ((\_SB.BMLF != 0x01))
                        {
                            If (((\_SB.OSYS < 0x07D6) || (\_SB.VDRV == 0x00)))
                            {
                                \_SB.SECS (0xA7)
                            }
                        }
                    }

                    Method (_Q52, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        P8XH (0x00, 0x31)
                        Notify (\_SB.ADP1, 0x80) // Status Change
                        PWRS = 0x00
                        TRAP (0x2B)
                        PNOT ()
                        If ((\_SB.BMLF != 0x01))
                        {
                            If (((\_SB.OSYS < 0x07D6) || (\_SB.VDRV == 0x00)))
                            {
                                \_SB.SECS (0xA7)
                            }
                        }
                    }

                    Method (_Q53, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        P8XH (0x00, 0x33)
                        Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
                        Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x81) // Information Change
                        PNOT ()
                    }

                    Method (_Q54, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        P8XH (0x00, 0x33)
                        Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
                        Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x81) // Information Change
                        PNOT ()
                    }

                    Method (_Q55, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        PNOT ()
                    }

                    Method (_Q5B, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        Notify (\_SB.SLPB, 0x80) // Status Change
                    }

                    Method (_Q5D, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        If (\IGDS)
                        {
                            TLST = 0x02
                        }

                        HKDS (0x0A)
                    }

                    Method (_Q5E, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \LIDS = LIDS /* \_SB_.PCI0.LPC0.H_EC.LIDS */
                        Notify (\_SB.LID0, 0x80) // Status Change
                    }

                    Method (_Q5F, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \LIDS = LIDS /* \_SB_.PCI0.LPC0.H_EC.LIDS */
                        Notify (\_SB.LID0, 0x80) // Status Change
                    }

                    Method (_Q60, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        If ((B1EX == 0x01))
                        {
                            Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
                        }
                    }

                    Method (_Q61, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        If ((B1EX == 0x01))
                        {
                            Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
                        }
                    }

                    Method (_Q63, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x88)
                        If ((OSYS >= 0x07D6))
                        {
                            \_SB.PCI0.IGD0.BRTN (0x87)
                        }
                    }

                    Method (_Q64, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x89)
                        If ((OSYS >= 0x07D6))
                        {
                            \_SB.PCI0.IGD0.BRTN (0x86)
                        }
                    }

                    Method (_Q65, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        Notify (\_TZ.TZ00, 0x80) // Thermal Status Change
                        \_SB.SECS (0xA9)
                    }

                    Method (_Q66, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        If ((B1EX == 0x01))
                        {
                            Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
                        }
                    }

                    Method (_Q68, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x8A)
                    }

                    Method (_Q69, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x8B)
                    }

                    Method (_Q70, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0xAB)
                        PNOT ()
                    }

                    Method (_Q73, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.GSSR = 0x01
                        If ((TIST <= 0x02))
                        {
                            Local0 = TIST /* \_SB_.PCI0.LPC0.H_EC.TIST */
                            \_SB.TZON = Local0
                            Local1 = \_SB.DIAG
                            If (Local1)
                            {
                                Sleep (0x64)
                            }
                            Else
                            {
                                CPRN ()
                            }
                        }
                        Else
                        {
                            \_SB.TZON = 0x00
                            CPRN ()
                        }

                        \_SB.SECS (0xA9)
                    }

                    Method (_Q76, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x94)
                    }

                    Method (_Q77, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x95)
                    }

                    Method (_Q79, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x8E)
                    }

                    Method (_Q7A, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x8F)
                    }

                    Method (_Q7D, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x92)
                    }

                    Method (_Q7E, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0x93)
                    }

                    Method (_Q7F, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        \_SB.SECS (0xB8)
                    }

                    Method (_Q80, 0, NotSerialized)  // _Qxx: EC Query, xx=0x00-0xFF
                    {
                        BreakPoint
                    }
                }

                Scope (\_SB)
                {
                    Device (ADP1)
                    {
                        Name (_HID, "ACPI0003" /* Power Source Device */)  // _HID: Hardware ID
                        Method (_PSR, 0, NotSerialized)  // _PSR: Power Source
                        {
                            If ((\ECON == 0x00))
                            {
                                Local0 = \_SB.SECB (0x85, 0x02)
                            }
                            Else
                            {
                                Local0 = \_SB.PCI0.LPC0.H_EC.ACEX
                            }

                            Return (Local0)
                        }

                        Method (_PCL, 0, NotSerialized)  // _PCL: Power Consumer List
                        {
                            Return (\_SB)
                        }

                        Method (_STA, 0, NotSerialized)  // _STA: Status
                        {
                            If ((\ECON == 0x00))
                            {
                                Local0 = 0x0F
                            }
                            ElseIf ((\_SB.PCI0.LPC0.H_EC.ACEX == 0x00))
                            {
                                Local0 = 0x1F
                            }
                            Else
                            {
                                Local0 = 0x0F
                            }

                            Local0 = 0x0F
                            Return (Local0)
                        }
                    }

                    Device (LID0)
                    {
                        Name (_HID, EisaId ("PNP0C0D") /* Lid Device */)  // _HID: Hardware ID
                        Method (_LID, 0, NotSerialized)  // _LID: Lid Status
                        {
                            Return (LIDS) /* \LIDS */
                        }
                    }

                    Device (PWRB)
                    {
                        Name (_HID, EisaId ("PNP0C0C") /* Power Button Device */)  // _HID: Hardware ID
                        Name (_PRW, Package (0x02)  // _PRW: Power Resources for Wake
                        {
                            0x1D, 
                            0x04
                        })
                    }

                    Device (SLPB)
                    {
                        Name (_HID, EisaId ("PNP0C0E") /* Sleep Button Device */)  // _HID: Hardware ID
                    }
                }

                Method (DECD, 4, Serialized)
                {
                    Debug = Arg0
                }

                Device (MBRD)
                {
                    Name (_HID, EisaId ("PNP0C02") /* PNP Motherboard Resources */)  // _HID: Hardware ID
                    Name (_UID, 0x1F)  // _UID: Unique ID
                    Name (RSRC, ResourceTemplate ()
                    {
                        IO (Decode16,
                            0x0010,             // Range Minimum
                            0x0010,             // Range Maximum
                            0x01,               // Alignment
                            0x10,               // Length
                            )
                        IO (Decode16,
                            0x0024,             // Range Minimum
                            0x0024,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0028,             // Range Minimum
                            0x0028,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x002C,             // Range Minimum
                            0x002C,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0030,             // Range Minimum
                            0x0030,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0034,             // Range Minimum
                            0x0034,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0038,             // Range Minimum
                            0x0038,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x003C,             // Range Minimum
                            0x003C,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0072,             // Range Minimum
                            0x0072,             // Range Maximum
                            0x01,               // Alignment
                            0x06,               // Length
                            )
                        IO (Decode16,
                            0x0080,             // Range Minimum
                            0x0080,             // Range Maximum
                            0x01,               // Alignment
                            0x01,               // Length
                            )
                        IO (Decode16,
                            0x0090,             // Range Minimum
                            0x0090,             // Range Maximum
                            0x01,               // Alignment
                            0x10,               // Length
                            )
                        IO (Decode16,
                            0x00A4,             // Range Minimum
                            0x00A4,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x00A8,             // Range Minimum
                            0x00A8,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x00AC,             // Range Minimum
                            0x00AC,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x00B0,             // Range Minimum
                            0x00B0,             // Range Maximum
                            0x01,               // Alignment
                            0x06,               // Length
                            )
                        IO (Decode16,
                            0x00B8,             // Range Minimum
                            0x00B8,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x00BC,             // Range Minimum
                            0x00BC,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x0800,             // Range Minimum
                            0x0800,             // Range Maximum
                            0x01,               // Alignment
                            0x10,               // Length
                            )
                        IO (Decode16,
                            0x1000,             // Range Minimum
                            0x1000,             // Range Maximum
                            0x01,               // Alignment
                            0x80,               // Length
                            _Y0F)
                        IO (Decode16,
                            0x1180,             // Range Minimum
                            0x1180,             // Range Maximum
                            0x01,               // Alignment
                            0x40,               // Length
                            _Y10)
                        IO (Decode16,
                            0x002E,             // Range Minimum
                            0x002E,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x04D0,             // Range Minimum
                            0x04D0,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0xFE00,             // Range Minimum
                            0xFE00,             // Range Maximum
                            0x01,               // Alignment
                            0x01,               // Length
                            )
                        IO (Decode16,
                            0x164E,             // Range Minimum
                            0x164E,             // Range Maximum
                            0x01,               // Alignment
                            0xFF,               // Length
                            )
                        Memory32Fixed (ReadWrite,
                            0xE0000000,         // Address Base
                            0x10000000,         // Address Length
                            )
                        Memory32Fixed (ReadWrite,
                            0xFED14000,         // Address Base
                            0x00004000,         // Address Length
                            )
                        Memory32Fixed (ReadWrite,
                            0xF8000000,         // Address Base
                            0x04000000,         // Address Length
                            )
                        Memory32Fixed (ReadWrite,
                            0xFEF00000,         // Address Base
                            0x00100000,         // Address Length
                            )
                    })
                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.MBRD._Y0F._MIN, PMMN)  // _MIN: Minimum Base Address
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.MBRD._Y0F._MAX, PMMX)  // _MAX: Maximum Base Address
                        PMMN = (^^PMBA & 0xFF80)
                        PMMX = PMMN /* \_SB_.PCI0.LPC0.MBRD._CRS.PMMN */
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.MBRD._Y10._MIN, GPMN)  // _MIN: Minimum Base Address
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.MBRD._Y10._MAX, GPMX)  // _MAX: Maximum Base Address
                        GPMN = (^^GPBA & 0xFF80)
                        GPMX = GPMN /* \_SB_.PCI0.LPC0.MBRD._CRS.GPMN */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.MBRD.RSRC */
                    }
                }

                Device (DMAC)
                {
                    Name (_HID, EisaId ("PNP0200") /* PC-class DMA Controller */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IO (Decode16,
                            0x0000,             // Range Minimum
                            0x0000,             // Range Maximum
                            0x01,               // Alignment
                            0x10,               // Length
                            )
                        IO (Decode16,
                            0x0081,             // Range Minimum
                            0x0081,             // Range Maximum
                            0x01,               // Alignment
                            0x0F,               // Length
                            )
                        IO (Decode16,
                            0x00C0,             // Range Minimum
                            0x00C0,             // Range Maximum
                            0x01,               // Alignment
                            0x20,               // Length
                            )
                        DMA (Compatibility, NotBusMaster, Transfer16, )
                            {4}
                    })
                }

                Device (MATH)
                {
                    Name (_HID, EisaId ("PNP0C04") /* x87-compatible Floating Point Processing Unit */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IO (Decode16,
                            0x00F0,             // Range Minimum
                            0x00F0,             // Range Maximum
                            0x01,               // Alignment
                            0x0F,               // Length
                            )
                        IRQ (Edge, ActiveHigh, Exclusive, )
                            {13}
                    })
                }

                Device (PIC)
                {
                    Name (_HID, EisaId ("PNP0000") /* 8259-compatible Programmable Interrupt Controller */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IO (Decode16,
                            0x0020,             // Range Minimum
                            0x0020,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x00A0,             // Range Minimum
                            0x00A0,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IRQ (Edge, ActiveHigh, Exclusive, )
                            {2}
                    })
                }

                Device (RTC)
                {
                    Name (_HID, EisaId ("PNP0B00") /* AT Real-Time Clock */)  // _HID: Hardware ID
                    Name (BUF0, ResourceTemplate ()
                    {
                        IO (Decode16,
                            0x0070,             // Range Minimum
                            0x0070,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                    })
                    Name (BUF1, ResourceTemplate ()
                    {
                        IO (Decode16,
                            0x0070,             // Range Minimum
                            0x0070,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IRQNoFlags ()
                            {8}
                    })
                    Method (_CRS, 0, Serialized)  // _CRS: Current Resource Settings
                    {
                        If ((MTSE & 0x00020000))
                        {
                            Return (BUF0) /* \_SB_.PCI0.LPC0.RTC_.BUF0 */
                        }

                        Return (BUF1) /* \_SB_.PCI0.LPC0.RTC_.BUF1 */
                    }
                }

                Device (SPKR)
                {
                    Name (_HID, EisaId ("PNP0800") /* Microsoft Sound System Compatible Device */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IO (Decode16,
                            0x0061,             // Range Minimum
                            0x0061,             // Range Maximum
                            0x01,               // Alignment
                            0x01,               // Length
                            )
                    })
                }

                Device (TIMR)
                {
                    Name (_HID, EisaId ("PNP0100") /* PC-class System Timer */)  // _HID: Hardware ID
                    Name (BUF0, ResourceTemplate ()
                    {
                        IO (Decode16,
                            0x0040,             // Range Minimum
                            0x0040,             // Range Maximum
                            0x01,               // Alignment
                            0x04,               // Length
                            )
                        IO (Decode16,
                            0x0050,             // Range Minimum
                            0x0050,             // Range Maximum
                            0x10,               // Alignment
                            0x04,               // Length
                            )
                    })
                    Name (BUF1, ResourceTemplate ()
                    {
                        IO (Decode16,
                            0x0040,             // Range Minimum
                            0x0040,             // Range Maximum
                            0x01,               // Alignment
                            0x04,               // Length
                            )
                        IO (Decode16,
                            0x0050,             // Range Minimum
                            0x0050,             // Range Maximum
                            0x10,               // Alignment
                            0x04,               // Length
                            )
                        IRQNoFlags ()
                            {0}
                    })
                    Method (_CRS, 0, Serialized)  // _CRS: Current Resource Settings
                    {
                        If ((MTSE & 0x00020000))
                        {
                            Return (BUF0) /* \_SB_.PCI0.LPC0.TIMR.BUF0 */
                        }

                        Return (BUF1) /* \_SB_.PCI0.LPC0.TIMR.BUF1 */
                    }
                }

                OperationRegion (MMTO, PCI_Config, 0xD0, 0x04)
                Field (MMTO, DWordAcc, NoLock, Preserve)
                {
                    MTSE,   32
                }

                Device (TPM)
                {
                    Method (_HID, 0, NotSerialized)  // _HID: Hardware ID
                    {
                        If ((TPMV == 0x01))
                        {
                            Return (0x0201D824)
                        }

                        If ((TPMV == 0x02))
                        {
                            Return (0x0435CF4D)
                        }

                        If ((TPMV == 0x03))
                        {
                            Return (0x02016D08)
                        }

                        If ((TPMV == 0x04))
                        {
                            Return (0x01016D08)
                        }

                        If (((TPMV == 0x05) || (TPMV == 0x06)))
                        {
                            Return (0x0010A35C)
                        }

                        If ((TPMV == 0x08))
                        {
                            Return (0x00128D06)
                        }

                        Return (0x310CD041)
                    }

                    Name (_CID, EisaId ("PNP0C31"))  // _CID: Compatible ID
                    Name (_UID, 0x01)  // _UID: Unique ID
                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If (TPRS)
                        {
                            Return (0x0F)
                        }

                        Return (0x00)
                    }

                    Name (BUF0, ResourceTemplate ()
                    {
                        Memory32Fixed (ReadWrite,
                            0xFED40000,         // Address Base
                            0x00005000,         // Address Length
                            )
                        IO (Decode16,
                            0x167E,             // Range Minimum
                            0x167E,             // Range Maximum
                            0x01,               // Alignment
                            0x02,               // Length
                            )
                        IO (Decode16,
                            0x1670,             // Range Minimum
                            0x1670,             // Range Maximum
                            0x01,               // Alignment
                            0x0C,               // Length
                            )
                    })
                    Name (BUF1, ResourceTemplate ()
                    {
                        Memory32Fixed (ReadWrite,
                            0xFED40000,         // Address Base
                            0x00005000,         // Address Length
                            )
                    })
                    Name (BUF2, ResourceTemplate ()
                    {
                        Memory32Fixed (ReadWrite,
                            0xFED40000,         // Address Base
                            0x00005000,         // Address Length
                            )
                    })
                    Method (_CRS, 0, Serialized)  // _CRS: Current Resource Settings
                    {
                        If ((TPMV == 0x01))
                        {
                            Return (BUF0) /* \_SB_.PCI0.LPC0.TPM_.BUF0 */
                        }

                        If (((TPMV == 0x05) || (TPMV == 0x06)))
                        {
                            Return (BUF2) /* \_SB_.PCI0.LPC0.TPM_.BUF2 */
                        }

                        Return (BUF1) /* \_SB_.PCI0.LPC0.TPM_.BUF1 */
                    }

                    Method (UCMP, 2, NotSerialized)
                    {
                        If ((0x10 != SizeOf (Arg0)))
                        {
                            Return (0x00)
                        }

                        If ((0x10 != SizeOf (Arg1)))
                        {
                            Return (0x00)
                        }

                        Local0 = 0x00
                        While ((Local0 < 0x10))
                        {
                            If ((DerefOf (Arg0 [Local0]) != DerefOf (Arg1 [Local0]
                                )))
                            {
                                Return (0x00)
                            }

                            Local0++
                        }

                        Return (0x01)
                    }

                    Method (_DSM, 4, Serialized)  // _DSM: Device-Specific Method
                    {
                        If ((UCMP (Arg0, ToUUID ("3dddfaa6-361b-4eb4-a424-8d10089d1653") /* Physical Presence Interface */) == 0x01))
                        {
                            If ((Arg2 == 0x00))
                            {
                                Return (Buffer (0x01)
                                {
                                     0x7F                                             // .
                                })
                            }

                            If ((Arg2 == 0x01))
                            {
                                Return (Buffer (0x04)
                                {
                                    "1.0"
                                })
                            }

                            If ((Arg2 == 0x02))
                            {
                                If (TPRS)
                                {
                                    If ((DerefOf (Arg3 [0x00]) == 0x00))
                                    {
                                        PPRQ = 0x00
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x01))
                                    {
                                        PPRQ = 0x01
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x02))
                                    {
                                        PPRQ = 0x02
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x03))
                                    {
                                        PPRQ = 0x03
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x04))
                                    {
                                        PPRQ = 0x04
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x05))
                                    {
                                        PPRQ = 0x05
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x06))
                                    {
                                        PPRQ = 0x06
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x07))
                                    {
                                        PPRQ = 0x07
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x08))
                                    {
                                        PPRQ = 0x08
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x09))
                                    {
                                        PPRQ = 0x09
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x0A))
                                    {
                                        PPRQ = 0x0A
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x0B))
                                    {
                                        PPRQ = 0x0B
                                        Return (0x00)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x0C))
                                    {
                                        PPRQ = 0x00
                                        Return (0x01)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x0D))
                                    {
                                        PPRQ = 0x00
                                        Return (0x01)
                                    }

                                    If ((DerefOf (Arg3 [0x00]) == 0x0E))
                                    {
                                        PPRQ = 0x0E
                                        Return (0x00)
                                    }

                                    Return (0x01)
                                }

                                Return (0x01)
                            }

                            If ((Arg2 == 0x03))
                            {
                                Name (TMP1, Package (0x02)
                                {
                                    0x00, 
                                    0xFFFFFFFF
                                })
                                TMP1 [0x01] = \_SB.PPRQ
                                Return (TMP1) /* \_SB_.PCI0.LPC0.TPM_._DSM.TMP1 */
                            }

                            If ((Arg2 == 0x04))
                            {
                                Return (0x01)
                            }

                            If ((Arg2 == 0x05))
                            {
                                Name (TMP2, Package (0x03)
                                {
                                    0x00, 
                                    0xFFFFFFFF, 
                                    0xFFFFFFFF
                                })
                                TMP2 [0x01] = \_SB.PPLO
                                If ((((\_SB.PPLO > 0x0E) || (\_SB.PPLO == 0x0C)) || (\_SB.PPLO == 0x0D)))
                                {
                                    TMP2 [0x02] = 0xFFFFFFF1
                                    Return (TMP2) /* \_SB_.PCI0.LPC0.TPM_._DSM.TMP2 */
                                }

                                If ((PPRP == 0xFF))
                                {
                                    TMP2 [0x02] = 0xFFFFFFF1
                                    Return (TMP2) /* \_SB_.PCI0.LPC0.TPM_._DSM.TMP2 */
                                }

                                If (PPOR)
                                {
                                    TMP2 [0x02] = 0xFFFFFFF0
                                    Return (TMP2) /* \_SB_.PCI0.LPC0.TPM_._DSM.TMP2 */
                                }

                                TMP2 [0x02] = \_SB.PPRP
                                Return (TMP2) /* \_SB_.PCI0.LPC0.TPM_._DSM.TMP2 */
                            }

                            If ((Arg2 == 0x06))
                            {
                                CreateByteField (Arg3, 0x04, LAN0)
                                CreateByteField (Arg3, 0x05, LAN1)
                                P80H = ((LAN1 << 0x08) | LAN0) /* \_SB_.PCI0.LPC0.TPM_._DSM.LAN0 */
                                If (((LAN0 == 0x65) || (LAN0 == 0x45)))
                                {
                                    If (((LAN1 == 0x6E) || (LAN1 == 0x4E)))
                                    {
                                        Return (0x00)
                                    }
                                }

                                Return (0x01)
                            }

                            Return (0x01)
                        }

                        If ((UCMP (Arg0, ToUUID ("376054ed-cc13-4675-901c-4756d7f2d45d") /* TPM Memory Clear */) == 0x01))
                        {
                            If ((Arg2 == 0x00))
                            {
                                Return (Buffer (0x01)
                                {
                                     0x01                                             // .
                                })
                            }

                            If ((Arg2 == 0x01))
                            {
                                If ((DerefOf (Arg3 [0x00]) == 0x00))
                                {
                                    \_SB.MOR = 0x00
                                    Return (0x00)
                                }

                                If ((DerefOf (Arg3 [0x00]) == 0x01))
                                {
                                    \_SB.MOR = 0x01
                                    Return (0x00)
                                }
                            }

                            Return (0x01)
                        }

                        Return (Buffer (0x01)
                        {
                             0x00                                             // .
                        })
                    }
                }

                Device (KBC0)
                {
                    Name (_HID, EisaId ("PNP0303") /* IBM Enhanced Keyboard (101/102-key, PS/2 Mouse) */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IO (Decode16,
                            0x0060,             // Range Minimum
                            0x0060,             // Range Maximum
                            0x01,               // Alignment
                            0x01,               // Length
                            )
                        IO (Decode16,
                            0x0064,             // Range Minimum
                            0x0064,             // Range Maximum
                            0x01,               // Alignment
                            0x01,               // Length
                            )
                        IRQ (Edge, ActiveHigh, Exclusive, )
                            {1}
                    })
                }

                Device (MSE0)
                {
                    Name (_HID, EisaId ("PNP0F13") /* PS/2 Mouse */)  // _HID: Hardware ID
                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If (\_SB.TOUA)
                        {
                            Return (0x0F)
                        }
                        Else
                        {
                            Return (0x00)
                        }
                    }

                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IRQ (Edge, ActiveHigh, Exclusive, )
                            {12}
                    })
                }

                Device (MSE1)
                {
                    Name (_HID, EisaId ("ETD0B00"))  // _HID: Hardware ID
                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If (\_SB.TOUA)
                        {
                            Return (0x00)
                        }
                        Else
                        {
                            Return (0x0F)
                        }
                    }

                    Name (_CID, Package (0x02)  // _CID: Compatible ID
                    {
                        EisaId ("SYN0002"), 
                        EisaId ("PNP0F13") /* PS/2 Mouse */
                    })
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        IRQ (Edge, ActiveHigh, Exclusive, )
                            {12}
                    })
                }

                Device (LNKA)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x01)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y11)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRA |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKA._Y11._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRA & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Debug = RSRC /* \_SB_.PCI0.LPC0.LNKA.RSRC */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKA.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        Debug = Arg0
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRA = (Local0 | (PIRA & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRA & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKB)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x02)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y12)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRB |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKB._Y12._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRB & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKB.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRB = (Local0 | (PIRB & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRB & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKC)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x03)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y13)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRC |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKC._Y13._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRC & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKC.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRC = (Local0 | (PIRC & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRC & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKD)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x04)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y14)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRD |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKD._Y14._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRD & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKD.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRD = (Local0 | (PIRD & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRD & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKE)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x05)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y15)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRE |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKE._Y15._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRE & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Debug = RSRC /* \_SB_.PCI0.LPC0.LNKE.RSRC */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKE.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        Debug = Arg0
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRE = (Local0 | (PIRE & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRE & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKF)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x06)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y16)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRF |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKF._Y16._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRF & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Debug = RSRC /* \_SB_.PCI0.LPC0.LNKF.RSRC */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKF.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        Debug = Arg0
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRF = (Local0 | (PIRF & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRF & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKG)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x07)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y17)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRG |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKG._Y17._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRG & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Debug = RSRC /* \_SB_.PCI0.LPC0.LNKG.RSRC */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKG.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        Debug = Arg0
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRG = (Local0 | (PIRG & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRG & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                Device (LNKH)
                {
                    Name (_HID, EisaId ("PNP0C0F") /* PCI Interrupt Link Device */)  // _HID: Hardware ID
                    Name (_UID, 0x08)  // _UID: Unique ID
                    Name (_PRS, ResourceTemplate ()  // _PRS: Possible Resource Settings
                    {
                        IRQ (Level, ActiveLow, Shared, )
                            {3,4,5,6,7,10,11,12,14,15}
                    })
                    Name (RSRC, ResourceTemplate ()
                    {
                        IRQ (Level, ActiveLow, Shared, _Y18)
                            {}
                    })
                    Method (_DIS, 0, NotSerialized)  // _DIS: Disable Device
                    {
                        PIRH |= 0x80
                    }

                    Method (_CRS, 0, NotSerialized)  // _CRS: Current Resource Settings
                    {
                        CreateWordField (RSRC, \_SB.PCI0.LPC0.LNKH._Y18._INT, IRQ0)  // _INT: Interrupts
                        Local0 = (PIRH & 0x0F)
                        IRQ0 = (0x01 << Local0)
                        Debug = RSRC /* \_SB_.PCI0.LPC0.LNKH.RSRC */
                        Return (RSRC) /* \_SB_.PCI0.LPC0.LNKH.RSRC */
                    }

                    Method (_SRS, 1, NotSerialized)  // _SRS: Set Resource Settings
                    {
                        Debug = Arg0
                        CreateWordField (Arg0, 0x01, IRQ0)
                        FindSetRightBit (IRQ0, Local0)
                        Local0--
                        PIRH = (Local0 | (PIRH & 0x70))
                    }

                    Method (_STA, 0, NotSerialized)  // _STA: Status
                    {
                        If ((PIRH & 0x80))
                        {
                            Return (0x09)
                        }

                        Return (0x0B)
                    }
                }

                OperationRegion (GPOX, SystemIO, 0x1180, 0x30)
                Field (GPOX, DWordAcc, NoLock, Preserve)
                {
                    Offset (0x07), 
                        ,   3, 
                    IO27,   1, 
                    Offset (0x0F), 
                        ,   3, 
                    LV27,   1, 
                    Offset (0x1B), 
                        ,   3, 
                    BL27,   1
                }

                OperationRegion (PIRX, PCI_Config, 0x60, 0x04)
                Field (PIRX, DWordAcc, Lock, Preserve)
                {
                    AccessAs (ByteAcc, 0x00), 
                    PIRA,   8, 
                    PIRB,   8, 
                    PIRC,   8, 
                    PIRD,   8
                }

                OperationRegion (PIRY, PCI_Config, 0x68, 0x04)
                Field (PIRY, DWordAcc, Lock, Preserve)
                {
                    AccessAs (ByteAcc, 0x00), 
                    PIRE,   8, 
                    PIRF,   8, 
                    PIRG,   8, 
                    PIRH,   8
                }

                OperationRegion (ELR0, PCI_Config, 0xA0, 0x14)
                Field (ELR0, DWordAcc, Lock, Preserve)
                {
                        ,   9, 
                    PBLV,   1, 
                    Offset (0x10), 
                        ,   1, 
                    ELSS,   1, 
                        ,   1, 
                    ELST,   1, 
                    ELPB,   1, 
                    Offset (0x11), 
                        ,   1, 
                    ELLO,   1, 
                    ELGN,   2, 
                    ELYL,   2, 
                    ELBE,   1, 
                    ELIE,   1, 
                    ELSN,   1, 
                    ELOC,   1, 
                    Offset (0x13), 
                    ELSO,   1
                }

                OperationRegion (ROUT, SystemIO, 0xB8, 0x04)
                Field (ROUT, DWordAcc, Lock, Preserve)
                {
                    AccessAs (ByteAcc, 0x00), 
                    GPI0,   2, 
                    GPI1,   2, 
                    GPI2,   2, 
                    GPI3,   2, 
                    GPI4,   2, 
                    GPI5,   2, 
                    GPI6,   2, 
                    GPI7,   2, 
                    GPI8,   2, 
                    GPI9,   2, 
                    GP10,   2, 
                    GP11,   2, 
                    GP12,   2, 
                    GP13,   2, 
                    GP14,   2, 
                    GP15,   2
                }

                OperationRegion (PMIO, SystemIO, 0x1000, 0x30)
                Field (PMIO, WordAcc, NoLock, Preserve)
                {
                    AccessAs (DWordAcc, 0x00), 
                    Offset (0x2D), 
                        ,   4, 
                    GPES,   1, 
                    Offset (0x2F), 
                        ,   4, 
                    GPEE,   1
                }

                OperationRegion (REGS, PCI_Config, 0x40, 0x10)
                Field (REGS, DWordAcc, Lock, Preserve)
                {
                    PMBA,   16, 
                    Offset (0x08), 
                    GPBA,   16
                }

                Device (FWH)
                {
                    Name (_HID, EisaId ("INT0800") /* Intel 82802 Firmware Hub Device */)  // _HID: Hardware ID
                    Name (_CRS, ResourceTemplate ()  // _CRS: Current Resource Settings
                    {
                        Memory32Fixed (ReadOnly,
                            0xFF800000,         // Address Base
                            0x00800000,         // Address Length
                            )
                    })
                }
            }

            Name (NATA, Package (0x01)
            {
                0x001F0002
            })
            Method (GETP, 1, NotSerialized)
            {
                Noop
                If (((Arg0 & 0x09) == 0x00))
                {
                    Return (0xFFFFFFFF)
                }

                If (((Arg0 & 0x09) == 0x08))
                {
                    Return (0x0384)
                }

                Local0 = ((Arg0 & 0x0300) >> 0x08)
                Local1 = ((Arg0 & 0x3000) >> 0x0C)
                Return ((0x1E * (0x09 - (Local0 + Local1))))
            }

            Method (GETD, 4, NotSerialized)
            {
                Noop
                If (Arg0)
                {
                    If (Arg1)
                    {
                        Return (0x14)
                    }

                    If (Arg2)
                    {
                        Return (((0x04 - Arg3) * 0x0F))
                    }

                    Return (((0x04 - Arg3) * 0x1E))
                }

                Return (0xFFFFFFFF)
            }

            Method (GETT, 1, NotSerialized)
            {
                Noop
                Return ((0x1E * (0x09 - (((Arg0 >> 0x02) & 0x03
                    ) + (Arg0 & 0x03)))))
            }

            Method (GETF, 3, NotSerialized)
            {
                Noop
                Name (TMPF, 0x00)
                If (Arg0)
                {
                    TMPF |= 0x01
                }

                If ((Arg2 & 0x02))
                {
                    TMPF |= 0x02
                }

                If (Arg1)
                {
                    TMPF |= 0x04
                }

                If ((Arg2 & 0x20))
                {
                    TMPF |= 0x08
                }

                If ((Arg2 & 0x4000))
                {
                    TMPF |= 0x10
                }

                Return (TMPF) /* \_SB_.PCI0.GETF.TMPF */
            }

            Method (SETP, 3, NotSerialized)
            {
                Noop
                If ((Arg0 >= 0xF0))
                {
                    Return (0x08)
                }
                Else
                {
                    If ((Arg1 & 0x02))
                    {
                        If (((Arg0 <= 0x78) && (Arg2 & 0x02)))
                        {
                            Return (0x2301)
                        }

                        If (((Arg0 <= 0xB4) && (Arg2 & 0x01)))
                        {
                            Return (0x2101)
                        }
                    }

                    Return (0x1001)
                }
            }

            Method (SETD, 1, NotSerialized)
            {
                Noop
                If ((Arg0 <= 0x14))
                {
                    Return (0x01)
                }

                If ((Arg0 <= 0x1E))
                {
                    Return (0x02)
                }

                If ((Arg0 <= 0x2D))
                {
                    Return (0x01)
                }

                If ((Arg0 <= 0x3C))
                {
                    Return (0x02)
                }

                If ((Arg0 <= 0x5A))
                {
                    Return (0x01)
                }

                Return (0x00)
            }

            Method (SETT, 3, NotSerialized)
            {
                Noop
                If ((Arg1 & 0x02))
                {
                    If (((Arg0 <= 0x78) && (Arg2 & 0x02)))
                    {
                        Return (0x0B)
                    }

                    If (((Arg0 <= 0xB4) && (Arg2 & 0x01)))
                    {
                        Return (0x09)
                    }
                }

                Return (0x04)
            }

            Device (IDE1)
            {
                Name (_ADR, 0x001F0002)  // _ADR: Address
                OperationRegion (IDEP, PCI_Config, 0x10, 0x02)
                Field (IDEP, DWordAcc, NoLock, Preserve)
                {
                    PCMD,   16
                }

                OperationRegion (IDES, PCI_Config, 0x18, 0x02)
                Field (IDES, DWordAcc, NoLock, Preserve)
                {
                    SCMD,   16
                }

                OperationRegion (IDEC, PCI_Config, 0x40, 0x18)
                Field (IDEC, DWordAcc, NoLock, Preserve)
                {
                    PRIT,   16, 
                    SECT,   16, 
                    PSIT,   4, 
                    SSIT,   4, 
                    Offset (0x08), 
                    SDMA,   4, 
                    Offset (0x0A), 
                    SDT0,   2, 
                        ,   2, 
                    SDT1,   2, 
                    Offset (0x0B), 
                    SDT2,   2, 
                        ,   2, 
                    SDT3,   2, 
                    Offset (0x14), 
                    ICR0,   4, 
                    ICR1,   4, 
                    ICR2,   4, 
                    ICR3,   4, 
                    ICR4,   4, 
                    ICR5,   4
                }

                OperationRegion (IDE1, PCI_Config, 0x90, 0x03)
                Field (IDE1, DWordAcc, NoLock, Preserve)
                {
                    MAP,    8, 
                    Offset (0x02), 
                    PCS,    8
                }

                OperationRegion (PBIO, SystemIO, 0x000018EF, 0x00000008)
                Field (PBIO, ByteAcc, NoLock, Preserve)
                {
                    Offset (0x07), 
                        ,   7, 
                    PBSY,   1
                }

                OperationRegion (SBIO, SystemIO, 0x000018EF, 0x00000008)
                Field (SBIO, ByteAcc, NoLock, Preserve)
                {
                    Offset (0x07), 
                        ,   7, 
                    SBSY,   1
                }

                Method (BSSP, 1, NotSerialized)
                {
                    If ((0x01 == \SPNF))
                    {
                        Local0 = (0x50 & PCS)
                        Local1 = (0xA0 & PCS)
                        Local2 = 0x00
                        Local3 = 0x00
                        If (Arg0)
                        {
                            If ((0x80 == Local1))
                            {
                                While ((SBSY && (0x4B > Local3)))
                                {
                                    Sleep (0x64)
                                    Local3++
                                }
                            }
                        }
                        ElseIf ((0x40 == Local0))
                        {
                            While ((PBSY && (0x4B > Local2)))
                            {
                                Sleep (0x64)
                                Local2++
                            }
                        }

                        \SPNF = 0x00
                    }
                }

                Method (CTYP, 1, NotSerialized)
                {
                    Local0 = Zero
                    If (Arg0)
                    {
                        If (((MAP > 0x01) && (MAP < 0x06)))
                        {
                            Local0 = 0x01
                        }
                        Else
                        {
                            If ((MAP == Zero))
                            {
                                Local0 = 0x03
                            }

                            If ((MAP == One))
                            {
                                Local0 = 0x04
                            }
                        }
                    }
                    ElseIf ((MAP > 0x05))
                    {
                        Local0 = 0x02
                    }
                    Else
                    {
                        If ((MAP == Zero))
                        {
                            Local0 = 0x05
                        }

                        If ((MAP == One))
                        {
                            Local0 = 0x06
                        }
                    }

                    Return (Local0)
                }

                Device (PRID)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Method (_GTM, 0, NotSerialized)  // _GTM: Get Timing Mode
                    {
                        Noop
                        Name (PBUF, Buffer (0x14)
                        {
                            /* 0000 */  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // ........
                            /* 0008 */  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // ........
                            /* 0010 */  0x00, 0x00, 0x00, 0x00                           // ....
                        })
                        CreateDWordField (PBUF, 0x00, PIO0)
                        CreateDWordField (PBUF, 0x04, DMA0)
                        CreateDWordField (PBUF, 0x08, PIO1)
                        CreateDWordField (PBUF, 0x0C, DMA1)
                        CreateDWordField (PBUF, 0x10, FLAG)
                        PIO0 = GETP (PRIT)
                        DMA0 = GETD ((SDMA & 0x01), (ICR3 & 0x01), (
                            ICR0 & 0x01), SDT0)
                        If ((DMA0 == 0xFFFFFFFF))
                        {
                            DMA0 = PIO0 /* \_SB_.PCI0.IDE1.PRID._GTM.PIO0 */
                        }

                        If ((PRIT & 0x4000))
                        {
                            If (((PRIT & 0x90) == 0x80))
                            {
                                PIO1 = 0x0384
                            }
                            Else
                            {
                                PIO1 = GETT (PSIT)
                            }
                        }
                        Else
                        {
                            PIO1 = 0xFFFFFFFF
                        }

                        DMA1 = GETD ((SDMA & 0x02), (ICR3 & 0x02), (
                            ICR0 & 0x02), SDT1)
                        If ((DMA1 == 0xFFFFFFFF))
                        {
                            DMA1 = PIO1 /* \_SB_.PCI0.IDE1.PRID._GTM.PIO1 */
                        }

                        FLAG = GETF ((SDMA & 0x01), (SDMA & 0x02), PRIT)
                        Return (PBUF) /* \_SB_.PCI0.IDE1.PRID._GTM.PBUF */
                    }

                    Method (_STM, 3, NotSerialized)  // _STM: Set Timing Mode
                    {
                        Noop
                        CreateDWordField (Arg0, 0x00, PIO0)
                        CreateDWordField (Arg0, 0x04, DMA0)
                        CreateDWordField (Arg0, 0x08, PIO1)
                        CreateDWordField (Arg0, 0x0C, DMA1)
                        CreateDWordField (Arg0, 0x10, FLAG)
                        ICR2 = 0x04
                        If ((SizeOf (Arg1) == 0x0200))
                        {
                            PRIT &= 0x4CF0
                            SDMA &= 0x0E
                            SDT0 = 0x00
                            ICR0 &= 0x0E
                            ICR1 &= 0x0E
                            ICR3 &= 0x0E
                            ICR5 &= 0x0E
                            CreateWordField (Arg1, 0x62, W490)
                            CreateWordField (Arg1, 0x6A, W530)
                            CreateWordField (Arg1, 0x7E, W630)
                            CreateWordField (Arg1, 0x80, W640)
                            CreateWordField (Arg1, 0xB0, W880)
                            PRIT |= 0x8004
                            If (((FLAG & 0x02) && (W490 & 0x0800)))
                            {
                                PRIT |= 0x02
                            }

                            PRIT |= SETP (PIO0, W530, W640)
                            If ((FLAG & 0x01))
                            {
                                SDMA |= 0x01
                                SDT0 = SETD (DMA0)
                                If ((W880 & 0x20))
                                {
                                    ICR1 |= 0x01
                                    ICR5 |= 0x01
                                }

                                If ((W880 & 0x10))
                                {
                                    ICR1 |= 0x01
                                }

                                If ((DMA0 < 0x1E))
                                {
                                    ICR3 |= 0x01
                                }

                                If ((DMA0 < 0x3C))
                                {
                                    ICR0 |= 0x01
                                }
                            }
                        }

                        If ((SizeOf (Arg2) == 0x0200))
                        {
                            PRIT &= 0x3F0F
                            PSIT = 0x00
                            SDMA &= 0x0D
                            SDT1 = 0x00
                            ICR0 &= 0x0D
                            ICR1 &= 0x0D
                            ICR3 &= 0x0D
                            ICR5 &= 0x0D
                            CreateWordField (Arg2, 0x62, W491)
                            CreateWordField (Arg2, 0x6A, W531)
                            CreateWordField (Arg2, 0x7E, W631)
                            CreateWordField (Arg2, 0x80, W641)
                            CreateWordField (Arg2, 0xB0, W881)
                            PRIT |= 0x8040
                            If (((FLAG & 0x08) && (W491 & 0x0800)))
                            {
                                PRIT |= 0x20
                            }

                            If ((FLAG & 0x10))
                            {
                                PRIT |= 0x4000
                                If ((PIO1 > 0xF0))
                                {
                                    PRIT |= 0x80
                                }
                                Else
                                {
                                    PRIT |= 0x10
                                    PSIT = SETT (PIO1, W531, W641)
                                }
                            }

                            If ((FLAG & 0x04))
                            {
                                SDMA |= 0x02
                                SDT1 = SETD (DMA1)
                                If ((W881 & 0x20))
                                {
                                    ICR1 |= 0x02
                                    ICR5 |= 0x02
                                }

                                If ((W881 & 0x10))
                                {
                                    ICR1 |= 0x02
                                }

                                If ((DMA0 < 0x1E))
                                {
                                    ICR3 |= 0x02
                                }

                                If ((DMA0 < 0x3C))
                                {
                                    ICR0 |= 0x02
                                }
                            }
                        }
                    }

                    Method (_PS0, 0, NotSerialized)  // _PS0: Power State 0
                    {
                        BSSP (0x00)
                    }

                    Method (_PS3, 0, NotSerialized)  // _PS3: Power State 3
                    {
                    }

                    Device (P_D0)
                    {
                        Name (_ADR, 0x00)  // _ADR: Address
                        Method (_GTF, 0, NotSerialized)  // _GTF: Get Task File
                        {
                            Noop
                            Name (PIB0, Buffer (0x15)
                            {
                                /* 0000 */  0x03, 0x00, 0x00, 0x00, 0x00, 0xA0, 0xEF, 0x03,  // ........
                                /* 0008 */  0x00, 0x00, 0x00, 0x00, 0xA0, 0xEF, 0x10, 0x03,  // ........
                                /* 0010 */  0x00, 0x00, 0x00, 0xA0, 0xEF                     // .....
                            })
                            CreateByteField (PIB0, 0x01, PMD0)
                            CreateByteField (PIB0, 0x08, DMD0)
                            If ((PRIT & 0x02))
                            {
                                If (((PRIT & 0x09) == 0x08))
                                {
                                    PMD0 = 0x08
                                }
                                Else
                                {
                                    PMD0 = 0x0A
                                    Local0 = ((PRIT & 0x0300) >> 0x08)
                                    Local1 = ((PRIT & 0x3000) >> 0x0C)
                                    Local2 = (Local0 + Local1)
                                    If ((0x03 == Local2))
                                    {
                                        PMD0 = 0x0B
                                    }

                                    If ((0x05 == Local2))
                                    {
                                        PMD0 = 0x0C
                                    }
                                }
                            }
                            Else
                            {
                                PMD0 = 0x01
                            }

                            If ((SDMA & 0x01))
                            {
                                DMD0 = (SDT0 | 0x40)
                                If ((ICR0 & 0x01))
                                {
                                    DMD0 += 0x02
                                }

                                If ((ICR3 & 0x01))
                                {
                                    DMD0 = 0x45
                                }
                            }
                            Else
                            {
                                DMD0 = (((PMD0 & 0x07) - 0x02) | 0x20)
                            }

                            Return (PIB0) /* \_SB_.PCI0.IDE1.PRID.P_D0._GTF.PIB0 */
                        }
                    }

                    Device (P_D1)
                    {
                        Name (_ADR, 0x01)  // _ADR: Address
                        Method (_GTF, 0, NotSerialized)  // _GTF: Get Task File
                        {
                            Noop
                            Name (PIB1, Buffer (0x15)
                            {
                                /* 0000 */  0x03, 0x00, 0x00, 0x00, 0x00, 0xB0, 0xEF, 0x03,  // ........
                                /* 0008 */  0x00, 0x00, 0x00, 0x00, 0xB0, 0xEF, 0x10, 0x03,  // ........
                                /* 0010 */  0x00, 0x00, 0x00, 0xA0, 0xEF                     // .....
                            })
                            CreateByteField (PIB1, 0x01, PMD1)
                            CreateByteField (PIB1, 0x08, DMD1)
                            If ((PRIT & 0x20))
                            {
                                If (((PRIT & 0x90) == 0x80))
                                {
                                    PMD1 = 0x08
                                }
                                Else
                                {
                                    Local0 = ((PSIT & 0x03) + ((PSIT & 0x0C) >> 0x02
                                        ))
                                    If ((0x05 == Local0))
                                    {
                                        PMD1 = 0x0C
                                    }
                                    ElseIf ((0x03 == Local0))
                                    {
                                        PMD1 = 0x0B
                                    }
                                    Else
                                    {
                                        PMD1 = 0x0A
                                    }
                                }
                            }
                            Else
                            {
                                PMD1 = 0x01
                            }

                            If ((SDMA & 0x02))
                            {
                                DMD1 = (SDT1 | 0x40)
                                If ((ICR0 & 0x02))
                                {
                                    DMD1 += 0x02
                                }

                                If ((ICR3 & 0x02))
                                {
                                    DMD1 = 0x45
                                }
                            }
                            Else
                            {
                                DMD1 = (((PMD1 & 0x07) - 0x02) | 0x20)
                            }

                            Return (PIB1) /* \_SB_.PCI0.IDE1.PRID.P_D1._GTF.PIB1 */
                        }
                    }
                }

                Device (SECD)
                {
                    Name (_ADR, 0x01)  // _ADR: Address
                    Method (_GTM, 0, NotSerialized)  // _GTM: Get Timing Mode
                    {
                        Noop
                        Name (SBUF, Buffer (0x14)
                        {
                            /* 0000 */  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // ........
                            /* 0008 */  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // ........
                            /* 0010 */  0x00, 0x00, 0x00, 0x00                           // ....
                        })
                        CreateDWordField (SBUF, 0x00, PIO0)
                        CreateDWordField (SBUF, 0x04, DMA0)
                        CreateDWordField (SBUF, 0x08, PIO1)
                        CreateDWordField (SBUF, 0x0C, DMA1)
                        CreateDWordField (SBUF, 0x10, FLAG)
                        PIO0 = GETP (SECT)
                        DMA0 = GETD ((SDMA & 0x04), (ICR3 & 0x04), (
                            ICR0 & 0x04), SDT2)
                        If ((DMA0 == 0xFFFFFFFF))
                        {
                            DMA0 = PIO0 /* \_SB_.PCI0.IDE1.SECD._GTM.PIO0 */
                        }

                        If ((SECT & 0x4000))
                        {
                            If (((SECT & 0x90) == 0x80))
                            {
                                PIO1 = 0x0384
                            }
                            Else
                            {
                                PIO1 = GETT (SSIT)
                            }
                        }
                        Else
                        {
                            PIO1 = 0xFFFFFFFF
                        }

                        DMA1 = GETD ((SDMA & 0x08), (ICR3 & 0x08), (
                            ICR0 & 0x08), SDT3)
                        If ((DMA1 == 0xFFFFFFFF))
                        {
                            DMA1 = PIO1 /* \_SB_.PCI0.IDE1.SECD._GTM.PIO1 */
                        }

                        FLAG = GETF ((SDMA & 0x04), (SDMA & 0x08), SECT)
                        Return (SBUF) /* \_SB_.PCI0.IDE1.SECD._GTM.SBUF */
                    }

                    Method (_STM, 3, NotSerialized)  // _STM: Set Timing Mode
                    {
                        Noop
                        CreateDWordField (Arg0, 0x00, PIO0)
                        CreateDWordField (Arg0, 0x04, DMA0)
                        CreateDWordField (Arg0, 0x08, PIO1)
                        CreateDWordField (Arg0, 0x0C, DMA1)
                        CreateDWordField (Arg0, 0x10, FLAG)
                        ICR2 = 0x04
                        If ((SizeOf (Arg1) == 0x0200))
                        {
                            SECT &= 0x4CF0
                            SDMA &= 0x0B
                            SDT2 = 0x00
                            ICR0 &= 0x0B
                            ICR1 &= 0x0B
                            ICR3 &= 0x0B
                            ICR5 &= 0x0B
                            CreateWordField (Arg1, 0x62, W490)
                            CreateWordField (Arg1, 0x6A, W530)
                            CreateWordField (Arg1, 0x7E, W630)
                            CreateWordField (Arg1, 0x80, W640)
                            CreateWordField (Arg1, 0xB0, W880)
                            SECT |= 0x8004
                            If (((FLAG & 0x02) && (W490 & 0x0800)))
                            {
                                SECT |= 0x02
                            }

                            SECT |= SETP (PIO0, W530, W640)
                            If ((FLAG & 0x01))
                            {
                                SDMA |= 0x04
                                SDT2 = SETD (DMA0)
                                If ((W880 & 0x20))
                                {
                                    ICR1 |= 0x04
                                    ICR5 |= 0x04
                                }

                                If ((W880 & 0x10))
                                {
                                    ICR1 |= 0x04
                                }

                                If ((DMA0 < 0x1E))
                                {
                                    ICR3 |= 0x04
                                }

                                If ((DMA0 < 0x3C))
                                {
                                    ICR0 |= 0x04
                                }
                            }
                        }

                        If ((SizeOf (Arg2) == 0x0200))
                        {
                            SECT &= 0x3F0F
                            SSIT = 0x00
                            SDMA &= 0x07
                            SDT3 = 0x00
                            ICR0 &= 0x07
                            ICR1 &= 0x07
                            ICR3 &= 0x07
                            ICR5 &= 0x07
                            CreateWordField (Arg2, 0x62, W491)
                            CreateWordField (Arg2, 0x6A, W531)
                            CreateWordField (Arg2, 0x7E, W631)
                            CreateWordField (Arg2, 0x80, W641)
                            CreateWordField (Arg2, 0xB0, W881)
                            SECT |= 0x8040
                            If (((FLAG & 0x08) && (W491 & 0x0800)))
                            {
                                SECT |= 0x20
                            }

                            If ((FLAG & 0x10))
                            {
                                SECT |= 0x4000
                                If ((PIO1 > 0xF0))
                                {
                                    SECT |= 0x80
                                }
                                Else
                                {
                                    SECT |= 0x10
                                    SSIT = SETT (PIO1, W531, W641)
                                }
                            }

                            If ((FLAG & 0x04))
                            {
                                SDMA |= 0x08
                                SDT3 = SETD (DMA1)
                                If ((W881 & 0x20))
                                {
                                    ICR1 |= 0x08
                                    ICR5 |= 0x08
                                }

                                If ((W881 & 0x10))
                                {
                                    ICR1 |= 0x08
                                }

                                If ((DMA0 < 0x1E))
                                {
                                    ICR3 |= 0x08
                                }

                                If ((DMA0 < 0x3C))
                                {
                                    ICR0 |= 0x08
                                }
                            }
                        }
                    }

                    Method (_PS0, 0, NotSerialized)  // _PS0: Power State 0
                    {
                        BSSP (0x01)
                    }

                    Method (_PS3, 0, NotSerialized)  // _PS3: Power State 3
                    {
                    }

                    Device (S_D0)
                    {
                        Name (_ADR, 0x00)  // _ADR: Address
                        Method (_GTF, 0, NotSerialized)  // _GTF: Get Task File
                        {
                            Noop
                            Name (SIB0, Buffer (0x15)
                            {
                                /* 0000 */  0x03, 0x00, 0x00, 0x00, 0x00, 0xA0, 0xEF, 0x03,  // ........
                                /* 0008 */  0x00, 0x00, 0x00, 0x00, 0xA0, 0xEF, 0x10, 0x03,  // ........
                                /* 0010 */  0x00, 0x00, 0x00, 0xA0, 0xEF                     // .....
                            })
                            CreateByteField (SIB0, 0x01, PMD0)
                            CreateByteField (SIB0, 0x08, DMD0)
                            If ((SECT & 0x02))
                            {
                                If (((SECT & 0x09) == 0x08))
                                {
                                    PMD0 = 0x08
                                }
                                Else
                                {
                                    PMD0 = 0x0A
                                    Local0 = ((SECT & 0x0300) >> 0x08)
                                    Local1 = ((SECT & 0x3000) >> 0x0C)
                                    Local2 = (Local0 + Local1)
                                    If ((0x03 == Local2))
                                    {
                                        PMD0 = 0x0B
                                    }

                                    If ((0x05 == Local2))
                                    {
                                        PMD0 = 0x0C
                                    }
                                }
                            }
                            Else
                            {
                                PMD0 = 0x01
                            }

                            If ((SDMA & 0x04))
                            {
                                DMD0 = (SDT2 | 0x40)
                                If ((ICR0 & 0x04))
                                {
                                    DMD0 += 0x02
                                }

                                If ((ICR3 & 0x04))
                                {
                                    DMD0 = 0x45
                                }
                            }
                            Else
                            {
                                DMD0 = (((PMD0 & 0x07) - 0x02) | 0x20)
                            }

                            Return (SIB0) /* \_SB_.PCI0.IDE1.SECD.S_D0._GTF.SIB0 */
                        }
                    }

                    Device (S_D1)
                    {
                        Name (_ADR, 0x01)  // _ADR: Address
                        Method (_GTF, 0, NotSerialized)  // _GTF: Get Task File
                        {
                            Noop
                            Name (SIB1, Buffer (0x15)
                            {
                                /* 0000 */  0x03, 0x00, 0x00, 0x00, 0x00, 0xB0, 0xEF, 0x03,  // ........
                                /* 0008 */  0x00, 0x00, 0x00, 0x00, 0xB0, 0xEF, 0x10, 0x03,  // ........
                                /* 0010 */  0x00, 0x00, 0x00, 0xA0, 0xEF                     // .....
                            })
                            CreateByteField (SIB1, 0x01, PMD1)
                            CreateByteField (SIB1, 0x08, DMD1)
                            If ((SECT & 0x20))
                            {
                                If (((SECT & 0x90) == 0x80))
                                {
                                    PMD1 = 0x08
                                }
                                Else
                                {
                                    Local0 = ((SSIT & 0x03) + ((SSIT & 0x0C) >> 0x02
                                        ))
                                    If ((0x05 == Local0))
                                    {
                                        PMD1 = 0x0C
                                    }
                                    ElseIf ((0x03 == Local0))
                                    {
                                        PMD1 = 0x0B
                                    }
                                    Else
                                    {
                                        PMD1 = 0x0A
                                    }
                                }
                            }
                            Else
                            {
                                PMD1 = 0x01
                            }

                            If ((SDMA & 0x02))
                            {
                                DMD1 = (SDT3 | 0x40)
                                If ((ICR0 & 0x08))
                                {
                                    DMD1 += 0x02
                                }

                                If ((ICR3 & 0x08))
                                {
                                    DMD1 = 0x45
                                }
                            }
                            Else
                            {
                                DMD1 = (((PMD1 & 0x07) - 0x02) | 0x20)
                            }

                            Return (SIB1) /* \_SB_.PCI0.IDE1.SECD.S_D1._GTF.SIB1 */
                        }
                    }
                }
            }

            Device (SMBS)
            {
                Name (_ADR, 0x001F0003)  // _ADR: Address
            }

            Device (USB1)
            {
                Name (_ADR, 0x001D0000)  // _ADR: Address
                OperationRegion (USBO, PCI_Config, 0xC4, 0x04)
                Field (USBO, DWordAcc, Lock, Preserve)
                {
                    RSEN,   2
                }

                Name (UPRW, Package (0x02)
                {
                    0x03, 
                    0x03
                })
                Method (_PSW, 1, NotSerialized)  // _PSW: Power State Wake
                {
                    If (Arg0)
                    {
                        RSEN = 0x01
                    }
                    Else
                    {
                        RSEN = 0x00
                    }
                }

                Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
                {
                    Return (0x02)
                }

                Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
                {
                    Return (0x02)
                }

                Device (RHUB)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Device (PRT2)
                    {
                        Name (_ADR, 0x02)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }
                }
            }

            Device (USB2)
            {
                Name (_ADR, 0x001D0001)  // _ADR: Address
                OperationRegion (USBO, PCI_Config, 0xC4, 0x04)
                Field (USBO, DWordAcc, Lock, Preserve)
                {
                    RSEN,   2
                }

                Name (UPRW, Package (0x02)
                {
                    0x04, 
                    0x03
                })
                Method (_PSW, 1, NotSerialized)  // _PSW: Power State Wake
                {
                    If (Arg0)
                    {
                        RSEN = 0x01
                    }
                    Else
                    {
                        RSEN = 0x00
                    }
                }

                Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
                {
                    Return (0x02)
                }

                Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
                {
                    Return (0x02)
                }

                Device (RHUB)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Device (PRT2)
                    {
                        Name (_ADR, 0x02)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }
                }
            }

            Device (USB3)
            {
                Name (_ADR, 0x001D0002)  // _ADR: Address
                OperationRegion (USBO, PCI_Config, 0xC4, 0x04)
                Field (USBO, DWordAcc, Lock, Preserve)
                {
                    RSEN,   2
                }

                Name (UPRW, Package (0x02)
                {
                    0x0C, 
                    0x03
                })
                Method (_PSW, 1, NotSerialized)  // _PSW: Power State Wake
                {
                    If (Arg0)
                    {
                        RSEN = 0x03
                    }
                    Else
                    {
                        RSEN = 0x00
                    }
                }

                Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
                {
                    Return (0x02)
                }

                Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
                {
                    Return (0x02)
                }

                Device (RHUB)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Device (PRT2)
                    {
                        Name (_ADR, 0x02)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }
                }
            }

            Device (USB4)
            {
                Name (_ADR, 0x001D0003)  // _ADR: Address
                OperationRegion (USBO, PCI_Config, 0xC4, 0x04)
                Field (USBO, DWordAcc, Lock, Preserve)
                {
                    RSEN,   2
                }

                Name (EPRW, Package (0x02)
                {
                    0x0E, 
                    0x03
                })
                Method (_PSW, 1, NotSerialized)  // _PSW: Power State Wake
                {
                    If (Arg0)
                    {
                        RSEN = 0x03
                    }
                    Else
                    {
                        RSEN = 0x00
                    }
                }

                Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
                {
                    Return (0x02)
                }

                Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
                {
                    Return (0x02)
                }

                Device (RHUB)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Device (PRT2)
                    {
                        Name (_ADR, 0x02)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }
                }
            }

            Device (EUSB)
            {
                Name (_ADR, 0x001D0007)  // _ADR: Address
                Name (UPRW, Package (0x02)
                {
                    0x0D, 
                    0x03
                })
                Method (_S3D, 0, NotSerialized)  // _S3D: S3 Device State
                {
                    Return (0x02)
                }

                Method (_S4D, 0, NotSerialized)  // _S4D: S4 Device State
                {
                    Return (0x02)
                }

                Device (RHUB)
                {
                    Name (_ADR, 0x00)  // _ADR: Address
                    Device (PRT2)
                    {
                        Name (_ADR, 0x02)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }

                    Device (PRT4)
                    {
                        Name (_ADR, 0x04)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }

                    Device (PRT6)
                    {
                        Name (_ADR, 0x06)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }

                    Device (PRT8)
                    {
                        Name (_ADR, 0x08)  // _ADR: Address
                        Name (_UPC, Package (0x04)  // _UPC: USB Port Capabilities
                        {
                            0x00, 
                            0x00, 
                            0x00, 
                            0x00
                        })
                    }
                }
            }
        }
    }

    Scope (_SI)
    {
        Method (_SST, 1, NotSerialized)  // _SST: System Status
        {
            If ((Arg0 == 0x01)){}
            If ((Arg0 == 0x03))
            {
                If ((OSYS == 0x07CE))
                {
                    \_SB.BCMD = (Arg0 | 0x80)
                    \_SB.SMIC = Zero
                }

                If ((OSYS == 0x07CF))
                {
                    \_SB.BCMD = (Arg0 | 0x80)
                    \_SB.SMIC = Zero
                }
                Else
                {
                }
            }
        }
    }

    Method (P8XH, 2, Serialized)
    {
        If ((Arg0 == 0x00))
        {
            P80D = ((P80D & 0xFFFFFF00) | Arg1)
        }

        If ((Arg0 == 0x01))
        {
            P80D = ((P80D & 0xFFFF00FF) | (Arg1 << 0x08))
        }

        If ((Arg0 == 0x02))
        {
            P80D = ((P80D & 0xFF00FFFF) | (Arg1 << 0x10))
        }

        If ((Arg0 == 0x03))
        {
            P80D = ((P80D & 0x00FFFFFF) | (Arg1 << 0x18))
        }

        P80H = P80D /* \P80D */
    }

    Method (TRAP, 1, Serialized)
    {
        SMIF = Arg0
        TRP0 = 0x00
        Return (SMIF) /* \SMIF */
    }

    Method (BRTW, 1, Serialized)
    {
        Local1 = Arg0
        If ((ALSE == 0x02))
        {
            Local1 = ((ALAF * Arg0) / 0x64)
            If ((Local1 > 0x64))
            {
                Local1 = 0x64
            }
        }

        Local0 = ((0xFF * Local1) / 0x64)
        PRM0 = Local0
        If ((TRAP (0x12) == 0x00))
        {
            P8XH (0x02, Local0)
            BRTL = Arg0
        }
    }

    Method (GETB, 3, Serialized)
    {
        Local0 = (Arg0 * 0x08)
        Local1 = (Arg1 * 0x08)
        CreateField (Arg2, Local0, Local1, TBF3)
        Return (TBF3) /* \GETB.TBF3 */
    }

    Method (HKDS, 1, Serialized)
    {
        If ((0x00 == DSEN))
        {
            If ((TRAP (Arg0) == 0x00))
            {
                If ((CADL != PADL))
                {
                    PADL = CADL /* \CADL */
                    If (((OSYS > 0x07D0) || (OSYS < 0x07D6)))
                    {
                        Notify (\_SB.PCI0, 0x00) // Bus Check
                    }
                    Else
                    {
                        Notify (\_SB.PCI0.IGD0, 0x00) // Bus Check
                    }

                    Sleep (0x02EE)
                }

                Notify (\_SB.PCI0.IGD0, 0x80) // Status Change
            }
        }

        If ((0x01 == DSEN))
        {
            If ((TRAP (Arg0++) == 0x00))
            {
                Notify (\_SB.PCI0.IGD0, 0x81) // Information Change
            }
        }
    }

    Method (LSDS, 1, Serialized)
    {
        If (Arg0)
        {
            HKDS (0x0C)
        }
        Else
        {
            HKDS (0x0E)
        }

        If ((DSEN != 0x01))
        {
            Sleep (0x32)
            While ((DSEN == 0x02))
            {
                Sleep (0x32)
            }
        }
    }

    Method (PNOT, 0, Serialized)
    {
        If (MPEN)
        {
            If ((PDC0 & 0x08))
            {
                Notify (\_PR.CPU0, 0x80) // Performance Capability Change
                If ((PDC0 & 0x10))
                {
                    Sleep (0x64)
                    Notify (\_PR.CPU0, 0x81) // C-State Change
                }
            }

            If ((PDC1 & 0x08))
            {
                Notify (\_PR.CPU1, 0x80) // Performance Capability Change
                If ((PDC1 & 0x10))
                {
                    Sleep (0x64)
                    Notify (\_PR.CPU1, 0x81) // C-State Change
                }
            }

            If ((PDC2 & 0x08))
            {
                Notify (\_PR.CPU2, 0x80) // Performance Capability Change
                If ((PDC2 & 0x10))
                {
                    Sleep (0x64)
                    Notify (\_PR.CPU2, 0x81) // C-State Change
                }
            }

            If ((PDC3 & 0x08))
            {
                Notify (\_PR.CPU3, 0x80) // Performance Capability Change
                If ((PDC3 & 0x10))
                {
                    Sleep (0x64)
                    Notify (\_PR.CPU3, 0x81) // C-State Change
                }
            }
        }
        Else
        {
            Notify (\_PR.CPU0, 0x80) // Performance Capability Change
            Sleep (0x64)
            Notify (\_PR.CPU0, 0x81) // C-State Change
        }

        Notify (\_SB.PCI0.LPC0.H_EC.BAT1, 0x80) // Status Change
    }

    Method (CPRN, 0, Serialized)
    {
        SPPC ()
        If (MPEN)
        {
            If ((PDC0 & 0x08))
            {
                Notify (\_PR.CPU0, 0x80) // Performance Capability Change
                Sleep (0x64)
            }

            If ((PDC1 & 0x08))
            {
                Notify (\_PR.CPU1, 0x80) // Performance Capability Change
                Sleep (0x64)
            }

            If ((PDC2 & 0x08))
            {
                Notify (\_PR.CPU2, 0x80) // Performance Capability Change
                Sleep (0x64)
            }

            If ((PDC3 & 0x08))
            {
                Notify (\_PR.CPU3, 0x80) // Performance Capability Change
                Sleep (0x64)
            }
        }
        Else
        {
            Notify (\_PR.CPU0, 0x80) // Performance Capability Change
            Sleep (0x64)
        }
    }

    Method (SPPC, 0, NotSerialized)
    {
        \_SB.NIST = (\_SB.PPCM + 0x01)
        \_SB.PPCS = 0x00
        \_PR.CPU0._PPC = \_SB.PPCS
        If ((\_SB.GSSR == 0x01))
        {
            If ((\_SB.TZON == 0x01))
            {
                Divide (\_SB.PPCM, 0x02, Local1, Local2)
                \_SB.PPCS = Local2
                \_SB.NIST = (\_SB.PPCM - \_SB.PPCS)
                \_SB.NIST++
                \_PR.CPU0._PPC = \_SB.PPCS
            }

            If ((\_SB.TZON == 0x02))
            {
                \_SB.PPCS = \_SB.PPCM
                \_SB.NIST = 0x01
                \_PR.CPU0._PPC = \_SB.PPCS
            }
        }

        If ((\_SB.GSSR == 0x02))
        {
            \_SB.PPCS = (\_SB.PPCM - \_SB.RIST)
            \_PR.CPU0._PPC = \_SB.PPCS
        }
    }

    Method (CCRN, 0, Serialized)
    {
        If (MPEN)
        {
            If ((PDC0 & 0x10))
            {
                Notify (\_PR.CPU0, 0x81) // C-State Change
            }

            If ((PDC1 & 0x10))
            {
                Notify (\_PR.CPU1, 0x81) // C-State Change
            }

            If ((PDC2 & 0x10))
            {
                Notify (\_PR.CPU2, 0x81) // C-State Change
            }

            If ((PDC3 & 0x10))
            {
                Notify (\_PR.CPU3, 0x81) // C-State Change
            }
        }
        Else
        {
            Notify (\_PR.CPU0, 0x81) // C-State Change
        }
    }

    Scope (\_TZ)
    {
        ThermalZone (TZ00)
        {
            Method (_CRT, 0, Serialized)  // _CRT: Critical Temperature
            {
                Return ((0x0AAC + (\_SB.CRTT * 0x0A)))
            }

            Method (_SCP, 1, Serialized)  // _SCP: Set Cooling Policy
            {
                CTYP = Arg0
            }

            Method (_TMP, 0, Serialized)  // _TMP: Temperature
            {
                If (ECON)
                {
                    Local0 = \_SB.PCI0.LPC0.H_EC.CTMP
                    If ((Local0 != 0xFF))
                    {
                        Return ((0x0AAC + (Local0 * 0x0A)))
                    }
                    Else
                    {
                        Return (0x0C1C)
                    }
                }

                Return (0x0BB8)
            }

            Method (_PSL, 0, Serialized)  // _PSL: Passive List
            {
                If (MPEN)
                {
                    Return (Package (0x04)
                    {
                        \_PR.CPU0, 
                        \_PR.CPU1, 
                        \_PR.CPU2, 
                        \_PR.CPU3
                    })
                }

                Return (Package (0x01)
                {
                    \_PR.CPU0
                })
            }

            Method (_PSV, 0, Serialized)  // _PSV: Passive Temperature
            {
                Return ((0x0AAC + (\_SB.PSVT * 0x0A)))
            }

            Method (_TC1, 0, Serialized)  // _TC1: Thermal Constant 1
            {
                Return (\_SB.TC1V)
            }

            Method (_TC2, 0, Serialized)  // _TC2: Thermal Constant 2
            {
                Return (\_SB.TC2V)
            }

            Method (_TSP, 0, Serialized)  // _TSP: Thermal Sampling Period
            {
                Return (\_SB.TSPV)
            }
        }
    }

    Scope (\_SB)
    {
        OperationRegion (SNVS, SystemMemory, 0x3F5C0C7D, 0x000000FF)
        Field (SNVS, AnyAcc, NoLock, Preserve)
        {
            SECI,   8, 
            DB00,   8, 
            DW00,   16, 
            OSYS,   16, 
            Offset (0x20), 
            Offset (0x40), 
            BFCC,   16, 
            Offset (0x50), 
            PVFN,   8, 
            IGDS,   8, 
            TLST,   8, 
            CADL,   8, 
            PADL,   8, 
            CSTE,   16, 
            NSTE,   16, 
            SSTE,   16, 
            NDID,   8, 
            BRTL,   8, 
            Offset (0x60), 
            PSVT,   8, 
            TC1V,   8, 
            TC2V,   8, 
            TSPV,   8, 
            CRTT,   8, 
            ACTT,   8, 
            Offset (0x70), 
            MPEN,   8, 
            PPCS,   8, 
            PPCM,   8, 
            PCP0,   8, 
            PCP1,   8, 
            GSSR,   8, 
            DIAG,   8, 
            TZON,   8, 
            NIST,   8, 
            RIST,   8, 
            RCST,   8, 
            CCST,   8, 
            RCNT,   8, 
            C3SU,   8, 
            C1ON,   8, 
            BMLF,   8, 
            TEST,   8, 
            MDEL,   8, 
            BCMV,   8, 
            C4AC,   8, 
            TOUA,   8
        }

        Mutex (MSEC, 0x00)
        OperationRegion (SECT, SystemIO, 0x5000, 0x10)
        Field (SECT, ByteAcc, NoLock, Preserve)
        {
            TRPS,   8
        }

        Method (SECS, 1, Serialized)
        {
            Acquire (MSEC, 0xFFFF)
            SECI = Arg0
            TRPS = Zero
            Release (MSEC)
        }

        Method (SECB, 2, Serialized)
        {
            Acquire (MSEC, 0xFFFF)
            SECI = Arg0
            DB00 = Arg1
            TRPS = Zero
            Local0 = DB00 /* \_SB_.DB00 */
            Release (MSEC)
            Return (Local0)
        }

        Method (SECW, 3, Serialized)
        {
            Acquire (MSEC, 0xFFFF)
            SECI = Arg0
            DB00 = Arg1
            DW00 = Arg2
            TRPS = Zero
            Local1 = DW00 /* \_SB_.DW00 */
            Release (MSEC)
            Return (Local1)
        }

        Method (STRP, 2, Serialized)
        {
            Acquire (MSEC, 0xFFFF)
            SECI = Arg0
            PVFN = Arg1
            TRPS = Zero
            Release (MSEC)
            Return (SECI) /* \_SB_.SECI */
        }

        Method (SOST, 0, Serialized)
        {
            If (CondRefOf (_OSI, Local0))
            {
                \_SB.OSYS = 0x07D1
                If (\_OSI ("Windows 2009"))
                {
                    \_SB.OSYS = 0x07D9
                }
                ElseIf (\_OSI ("Windows 2006"))
                {
                    \_SB.OSYS = 0x07D6
                }
                ElseIf (\_OSI ("Windows 2001 SP3"))
                {
                    \_SB.OSYS = 0x07D2
                }
                ElseIf (\_OSI ("Windows 2001 SP2"))
                {
                    \_SB.OSYS = 0x07D2
                }
                ElseIf (\_OSI ("Windows 2001 SP1"))
                {
                    \_SB.OSYS = 0x07D2
                }
                ElseIf (\_OSI ("Windows 2009"))
                {
                    \_SB.OSYS = 0x07D9
                }

                \_SB.SECS (0x00)
            }
            ElseIf (((SizeOf (_OS) == 0x14) || (SizeOf (_OS) == 0x05)))
            {
                \_SB.OSYS = 0x07D0
            }
            ElseIf ((SizeOf (_OS) == 0x27))
            {
                \_SB.OSYS = 0x07CF
            }
            ElseIf ((SizeOf (_OS) == 0x12))
            {
                \_SB.OSYS = 0x07CE
            }
            Else
            {
                \_SB.OSYS = 0x07CD
            }
        }
    }

    Name (_S0, Package (0x02)  // _S0_: S0 System State
    {
        0x00, 
        0x00
    })
    Name (_S3, Package (0x02)  // _S3_: S3 System State
    {
        0x05, 
        0x05
    })
    Name (_S4, Package (0x02)  // _S4_: S4 System State
    {
        0x06, 
        0x06
    })
    Name (_S5, Package (0x02)  // _S5_: S5 System State
    {
        0x07, 
        0x07
    })
    Name (PICF, 0x00)
    Name (SPNF, 0x00)
    Method (_PIC, 1, NotSerialized)  // _PIC: Interrupt Model
    {
        \PICF = Arg0
    }

    Method (_PTS, 1, NotSerialized)  // _PTS: Prepare To Sleep
    {
        P80H = Arg0
        P80D = 0x00
        P8XH (0x00, Arg0)
        \_SB.BFCC = \_SB.PCI0.LPC0.H_EC.B1DF
        If ((Arg0 == 0x04))
        {
            \_SB.SECS (0xA5)
        }

        If ((Arg0 == 0x03))
        {
            \_SB.BCMD = 0x4C
            \_SB.SMIC = Zero
            \_SB.SECS (0xA4)
        }

        If ((Arg0 == 0x05))
        {
            \_SB.PHSR (0x4B)
        }
    }

    Method (_WAK, 1, NotSerialized)  // _WAK: Wake
    {
        \_SB.PCI0.PEXE = 0x00
        If ((Arg0 == 0x03))
        {
            \SPNF = 0x01
            TRAP (0x46)
            \_SB.SECS (0xB3)
            \_SB.SECB (0xB9, 0x02)
            P8XH (0x00, 0x03)
            Sleep (0xC8)
            \_SB.SECS (0xB5)
        }

        \PWRS = \_SB.PCI0.LPC0.H_EC.ACEX
        If ((Arg0 == 0x04))
        {
            \_SB.OSHT ()
            If (DTSE)
            {
                TRAP (0x47)
            }

            \_SB.SECB (0xB9, 0x01)
            P8XH (0x00, 0x04)
        }

        \_SB.SECS (0xAA)
        PNOT ()
        If ((OSYS == 0x07CE)){}
    }

    Scope (\_SB)
    {
        Name (OSTB, Ones)
        OperationRegion (OSTY, SystemMemory, 0x3F5C2EBC, 0x00000001)
        Field (OSTY, AnyAcc, NoLock, Preserve)
        {
            TPOS,   8
        }

        Method (OSTP, 0, NotSerialized)
        {
            If ((^OSTB == Ones))
            {
                If (CondRefOf (\_OSI, Local0))
                {
                    If (\_OSI ("Windows 2001.1"))
                    {
                        ^OSTB = 0x20
                        ^TPOS = 0x20
                    }
                    ElseIf (\_OSI ("Windows 2001 SP1"))
                    {
                        ^OSTB = 0x10
                        ^TPOS = 0x10
                    }
                    ElseIf (\_OSI ("Windows 2001"))
                    {
                        ^OSTB = 0x08
                        ^TPOS = 0x08
                    }
                    Else
                    {
                        ^OSTB = 0x00
                        ^TPOS = 0x00
                    }
                }
                ElseIf (CondRefOf (\_OS, Local0))
                {
                    If (^SEQL (\_OS, "Microsoft Windows"))
                    {
                        ^OSTB = 0x01
                        ^TPOS = 0x01
                    }
                    ElseIf (^SEQL (\_OS, "Microsoft WindowsME: Millennium Edition"))
                    {
                        ^OSTB = 0x02
                        ^TPOS = 0x02
                    }
                    ElseIf (^SEQL (\_OS, "Microsoft Windows NT"))
                    {
                        ^OSTB = 0x04
                        ^TPOS = 0x04
                    }
                    Else
                    {
                        ^OSTB = 0x00
                        ^TPOS = 0x00
                    }
                }
                Else
                {
                    ^OSTB = 0x00
                    ^TPOS = 0x00
                }
            }

            Return (^OSTB) /* \_SB_.OSTB */
        }

        Method (OSHT, 0, NotSerialized)
        {
            \_SB.OSTP ()
        }

        Method (SEQL, 2, Serialized)
        {
            Local0 = SizeOf (Arg0)
            Local1 = SizeOf (Arg1)
            If ((Local0 != Local1))
            {
                Return (Zero)
            }

            Name (BUF0, Buffer (Local0){})
            BUF0 = Arg0
            Name (BUF1, Buffer (Local0){})
            BUF1 = Arg1
            Local2 = Zero
            While ((Local2 < Local0))
            {
                Local3 = DerefOf (BUF0 [Local2])
                Local4 = DerefOf (BUF1 [Local2])
                If ((Local3 != Local4))
                {
                    Return (Zero)
                }

                Local2++
            }

            Return (One)
        }
    }

    Name (FWSO, "FWSO")
    Name (_PSC, 0x00)  // _PSC: Power State Current
    Method (_PS0, 0, NotSerialized)  // _PS0: Power State 0
    {
        Local0 = _PSC /* \_PSC */
        _PSC = 0x00
        If ((Local0 == 0x03))
        {
            \_SB.INF = 0x01
            While (\_SB.INF)
            {
                If (((\_SB.INF == 0x01) && (\_SB.OSTB >= 0x04)))
                {
                    Sleep (0x01F4)
                }
            }
        }
    }

    Method (_PS3, 0, NotSerialized)  // _PS3: Power State 3
    {
        _PSC = 0x03
    }
}

