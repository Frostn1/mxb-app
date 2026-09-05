#!/usr/bin/env python3
"""Trace exactly which bytes MX Bikes reads out of a `.map`, by emulating its loader.

The `.map` format is not documented and static reading of the loader kept producing
plausible-but-wrong answers — the sections are flag-gated and irregular, so a wrong guess
looks right for one record and drifts on the next. This runs the real loader instead.

It maps the unpacked `mxbikes.exe` into a Unicorn x86-64 emulator, starts at the function that
reads everything after the mesh (`0x14025f180`), and hooks the game's own read helper so every
call is serviced from a real `.map` on disk and logged as `(offset, size)`. Whatever comes out
*is* the format, for that file, with no inference.

Everything that cannot touch the file is stubbed: a function is skipped if neither it nor
anything it calls reaches the read helper, so imports, string handling and the rest never need
to work. Calls through pointers we never filled in are unwound with a shadow return stack.

    pip3 install unicorn pefile capstone
    python3 scripts/map-loader-trace.py [start-offset]

Needs `~/Downloads/mxbikes.exe.unpacked.exe` (see the notes on the unpacked binaries) and a
`.map` to read. The OEM drag strip is the reference worth using: its mesh, materials and
textures are all declared empty, so everything it contains is in the part still being decoded.

On the OEM drag strip it now walks the whole file: 14,545 reads ending at byte 120,609,745 of
120,609,745, exactly. That output is the format.

What made it work, after a long time not working: **only stub *primary* functions.** Half of
this binary's 8,741 `.pdata` entries are chained fragments — continuations of a function that
lives elsewhere — and treating one as a function entry means stubbing the middle of something
already running. It popped a return address that had never been pushed and execution left for
nowhere, which looked exactly like an unresolvable tail call and was blamed on one for a long
time.

Note the version word: the OEM strip and Indiana are 304, PiBoSo's `mxb_track_example` is 288,
and the record layout differs between them. Trace the version you intend to write.

The payload primitive is `0x1402573f0`: a 4-byte size, then 8 bytes, then `size - 8` bytes of
raw DEFLATE.
"""

import struct, os, sys
import pefile
import capstone
from unicorn import *
from unicorn.x86_const import *

EXE = os.path.expanduser("~/Downloads/mxbikes.exe.unpacked.exe")
MAP = os.environ.get("MAP") or os.path.expanduser(
    "~/Projects/pkz/L21-DragStrip_OEM/L21-DragStrip_OEM.map")

pe = pefile.PE(EXE, fast_load=True)
pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_EXCEPTION']])
_pdata = [(e.struct.BeginAddress, e.struct.EndAddress, e.struct.UnwindData)
          for e in pe.DIRECTORY_ENTRY_EXCEPTION]
BASE = pe.OPTIONAL_HEADER.ImageBase
img  = pe.get_memory_mapped_image(ImageBase=BASE)
IMGSZ = (len(img) + 0xFFF) & ~0xFFF

READ  = 0x140158a90     # read(handle, size, dest)
ALLOC = 0x14015b4f0     # alloc(size)
FREE  = 0x14015b540
MEMSET= 0x1402ab340
ENTRY = int(os.environ.get("ENTRY", "0x14025f180"), 16)

HEAP  = 0x200000000
STACK = 0x300000000
FILE  = 0x100000000     # not mapped; we service reads from python

mu = Uc(UC_ARCH_X86, UC_MODE_64)
mu.mem_map(BASE, IMGSZ + 0x1000)
mu.mem_write(BASE, bytes(img))
mu.mem_map(HEAP, 0x40000000)          # 1 GB scratch
mu.mem_map(STACK, 0x100000)
mu.mem_map(0, 0x100000)          # scratch for writes through null pointers

data = open(MAP,'rb').read()
state = {"cursor": int(os.environ.get("CURSOR", sys.argv[1] if len(sys.argv)>1 else 312)),
         "heap": HEAP + 0x1000, "log": [], "depth": 0, "overran": None}

_stub_targets = set()
_trail = []
def _unwind(uc):
    # Execution has left the image — a call through a pointer we never filled in. Find the
    # nearest return address on the stack and carry on as if that call returned 0.
    rsp = uc.reg_read(UC_X86_REG_RSP)
    for slot in range(0, 96):
        try:
            ret = struct.unpack('<Q', uc.mem_read(rsp + slot*8, 8))[0]
        except Exception:
            return False
        if BASE <= ret < BASE + IMGSZ:
            uc.reg_write(UC_X86_REG_RAX, 0)
            uc.reg_write(UC_X86_REG_RSP, rsp + (slot+1)*8)
            uc.reg_write(UC_X86_REG_RIP, ret)
            return True
    return False

# No shadow return stack.
#
# There was one, to recover from tail calls through pointers we never filled in. It relied on
# spotting every `ret`, and a linear disassembly of this binary finds 2,692 call sites but only
# 198 returns — data interleaved with code desyncs it. So the stack drifted, and an unwind
# landed execution in the middle of a function it had left long before, which invented a read
# of -7 bytes. An unwinder that is wrong is worse than none: it turns a stopped trace into a
# fictional one.
def hook_code(uc, addr, size, _):
    _trail.append(addr)
    if len(_trail) > 24: _trail.pop(0)
    if not (BASE <= addr < BASE + IMGSZ):
        if not _unwind(uc):
            rsp = uc.reg_read(UC_X86_REG_RSP)
            try:
                sl = struct.unpack('<8Q', uc.mem_read(rsp, 64))
            except Exception:
                sl = ()
            print(f"  [cannot unwind from {addr:#x}; rsp={rsp:#x} slots={[hex(x) for x in sl]}]")
            uc.emu_stop()
        return
    # First instruction of a function that never touches the file: skip the whole thing.
    if addr not in (READ, ALLOC, FREE, MEMSET, ENTRY):
        fn = _fn_of(addr)
        if fn and fn[0] == addr and addr not in _fragments and not reads_file(addr):
            _stub_targets.add(addr)
    if addr == READ:
        h  = uc.reg_read(UC_X86_REG_ECX)
        n  = uc.reg_read(UC_X86_REG_EDX) & 0xFFFFFFFF
        dst= uc.reg_read(UC_X86_REG_R8)
        off= state["cursor"]
        # Stop dead at the end of the file. This used to zero-fill, and zero-filling is how
        # you get a confident, detailed, entirely fictional trace: once the cursor runs off,
        # every count reads back as 0, every loop "terminates cleanly", and the log looks like
        # a decode. It cost me a wrong finding — "records 1 to 11 have no textures" — that was
        # nothing but zeros past the end.
        if off >= len(data) or off + n > len(data):
            state["overran"] = (off, n)
            uc.emu_stop()
            return
        chunk = data[off:off+n]
        try: uc.mem_write(dst, chunk)
        except Exception: pass
        try:
            _ret = struct.unpack('<Q', uc.mem_read(uc.reg_read(UC_X86_REG_RSP), 8))[0]
        except Exception:
            _ret = 0
        state["log"].append((off, n, dst, _ret))
        state["cursor"] += n
        # return 0 and emulate `ret`
        uc.reg_write(UC_X86_REG_RAX, 0)
        rsp = uc.reg_read(UC_X86_REG_RSP)
        uc.reg_write(UC_X86_REG_RIP, struct.unpack('<Q', uc.mem_read(rsp,8))[0])
        uc.reg_write(UC_X86_REG_RSP, rsp+8)
    elif addr in (ALLOC,):
        n = uc.reg_read(UC_X86_REG_ECX) & 0xFFFFFFFF
        p = state["heap"]
        state["heap"] = (p + max(n,16) + 0xFFF) & ~0xFFF
        uc.reg_write(UC_X86_REG_RAX, p)
        rsp = uc.reg_read(UC_X86_REG_RSP)
        uc.reg_write(UC_X86_REG_RIP, struct.unpack('<Q', uc.mem_read(rsp,8))[0])
        uc.reg_write(UC_X86_REG_RSP, rsp+8)
    elif addr not in (ENTRY,) and addr in _stub_targets:
        uc.reg_write(UC_X86_REG_RAX, 0)
        rsp = uc.reg_read(UC_X86_REG_RSP)
        uc.reg_write(UC_X86_REG_RIP, struct.unpack('<Q', uc.mem_read(rsp,8))[0])
        uc.reg_write(UC_X86_REG_RSP, rsp+8)
    elif addr in (FREE, MEMSET):
        if addr == MEMSET:
            d = uc.reg_read(UC_X86_REG_RCX); v = uc.reg_read(UC_X86_REG_EDX)&0xFF
            n = uc.reg_read(UC_X86_REG_R8) & 0xFFFFFF
            try: uc.mem_write(d, bytes([v])*n)
            except Exception: pass
        uc.reg_write(UC_X86_REG_RAX, 0)
        rsp = uc.reg_read(UC_X86_REG_RSP)
        uc.reg_write(UC_X86_REG_RIP, struct.unpack('<Q', uc.mem_read(rsp,8))[0])
        uc.reg_write(UC_X86_REG_RSP, rsp+8)

# Which functions actually read from the file, directly or through another that does.
# Everything else can be stubbed to "returned 0" without changing the byte layout, which is
# the only thing being measured here.
import capstone
# Only *primary* functions. Half of this binary's 8,741 .pdata entries are chained
# fragments — continuations of a function that lives elsewhere — and treating one as a
# function entry means stubbing the middle of something already running. That is what the
# "unresolved tail call to 0x0" was: `0x140257cad` is a fragment of the payload primitive,
# the stub popped a return address that was never pushed, and execution left for nowhere.
def _is_chained(unwind_rva):
    for sec in pe.sections:
        if sec.VirtualAddress <= unwind_rva < sec.VirtualAddress + sec.Misc_VirtualSize:
            off = sec.PointerToRawData + (unwind_rva - sec.VirtualAddress)
            return bool((_raw[off] >> 3) & 0x4)
    return False
_raw = open(EXE, 'rb').read()
_ranges = [(BASE+a, BASE+b) for a, b, u in _pdata if not _is_chained(u)]
_fragments = {BASE+a for a, b, u in _pdata if _is_chained(u)}
_md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
_calls = {}
def _fn_of(va):
    for a,b in _ranges:
        if a <= va < b: return (a,b)
    return None
def _direct_calls(fn):
    if fn in _calls: return _calls[fn]
    a,b = fn
    out=set()
    try:
        code = bytes(img[a-BASE:b-BASE])
        for ins in _md.disasm(code, a):
            if ins.mnemonic=="call" and ins.op_str.startswith("0x"):
                out.add(int(ins.op_str,16))
    except Exception:
        pass
    _calls[fn]=out
    return out
_reads_file = {}
def reads_file(va, depth=0):
    fn=_fn_of(va)
    if fn is None: return False
    if fn in _reads_file: return _reads_file[fn]
    if depth > 6:
        return False
    _reads_file[fn]=False           # break cycles
    r = READ in _direct_calls(fn) or any(reads_file(t, depth+1) for t in _direct_calls(fn))
    _reads_file[fn]=r
    return r

mu.hook_add(UC_HOOK_CODE, hook_code)

# Anything we haven't implemented — imports, vtable calls into unmapped memory — is treated as
# a function that returned 0. That keeps the walk going through the parts we don't care about;
# what we are after is the sequence of file reads, and those are hooked above.
def on_bad_fetch(uc, access, addr, size, value, _):
    # Landed somewhere that isn't code — an unresolved import, a vtable slot we never filled.
    # Walk up the stack for the nearest plausible return address and carry on from there as
    # if the call had returned 0.
    rsp = uc.reg_read(UC_X86_REG_RSP)
    for slot in range(0, 64):
        try:
            ret = struct.unpack('<Q', uc.mem_read(rsp + slot*8, 8))[0]
        except Exception:
            break
        if BASE <= ret < BASE + IMGSZ:
            uc.reg_write(UC_X86_REG_RAX, 0)
            uc.reg_write(UC_X86_REG_RSP, rsp + (slot+1)*8)
            uc.reg_write(UC_X86_REG_RIP, ret)
            return True
    return False
mu.hook_add(UC_HOOK_MEM_FETCH_UNMAPPED, on_bad_fetch)

_mapped = set()
def on_bad_mem(uc, access, addr, size, value, _):

    # Map, zero filled, whatever the code touches that we didn't plan for. One page at a
    # time and remembered, because mapping over an existing region is itself an error.
    # The null page is mapped too: a stray write through a pointer we never filled in should
    # land somewhere harmless rather than stop the run. Execution down there is still caught,
    # in hook_code, which is the case that actually matters.
    # Map a generous slab, skipping anything already mapped — the decompressor writes
    # megabytes and a page at a time just faults again on the next byte.
    SLAB = 0x400000
    start = addr & ~(SLAB-1)
    existing = [(r[0], r[1]) for r in uc.mem_regions()]
    for off in range(0, SLAB, 0x1000):
        pg = start + off
        if pg in _mapped: continue
        if any(a <= pg <= b for a,b in existing): 
            _mapped.add(pg); continue
        try:
            uc.mem_map(pg, 0x1000)
        except Exception:
            pass
        _mapped.add(pg)
    return True
mu.hook_add(UC_HOOK_MEM_READ_UNMAPPED | UC_HOOK_MEM_WRITE_UNMAPPED, on_bad_mem)

obj = HEAP           # the "rbx" object the loader fills in
mu.reg_write(UC_X86_REG_RCX, obj)
mu.reg_write(UC_X86_REG_RDX, 1)        # file handle
mu.reg_write(UC_X86_REG_R8, 304)       # version
sp = STACK + 0x80000
mu.reg_write(UC_X86_REG_RSP, sp)
mu.mem_write(sp, struct.pack('<Q', 0xdeadf00d))   # fake return address

try:
    mu.emu_start(ENTRY, 0xdeadf00d, timeout=0, count=80_000_000)
except UcError as e:
    print(f"stopped: {e} at rip={mu.reg_read(UC_X86_REG_RIP):x}")
except Exception as e:
    print("stopped:", e)

print("last addresses before stopping:", " -> ".join(hex(a) for a in _trail[-12:]))
log = state["log"]
print(f"{len(log)} reads, cursor ended at {state['cursor']:,} of {len(data):,}")
if state["overran"]:
    off, n = state["overran"]
    print(f"stopped at the end of the file: a read of {n:,} bytes at {off:,} "
          f"(file is {len(data):,}). Everything up to here is real; nothing past it would be.")
SZ = len(data)
for i,(o,n,d,ret) in enumerate(log):
    if o > SZ:
        print(f"  ... ran past the end of the file after {i} reads")
        break
    print(f"  {i:3}  off={o:>12,}  size={n:>10,}  from={ret:#x}")
