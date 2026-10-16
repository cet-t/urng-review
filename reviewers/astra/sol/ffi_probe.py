"""Exercise actual exported Windows C functions with initialized guarded buffers.
Run after cargo build --release --all-features --lib. Logs go to logs/ffi.json.
This does NOT establish validity of uninitialized Rust slices or other UB.
"""
import ctypes as C
import json
from pathlib import Path
import re
import subprocess

HERE=Path(__file__).resolve().parent
DLL=HERE/'target'/'release'/'urng.dll'
lib=C.CDLL(str(DLL))
exports=subprocess.check_output(['llvm-readobj','--coff-exports',str(DLL)],text=True)
names=set(re.findall(r'Name: (\w+)',exports))
cs=(HERE/'work'/'source'/'target'/'release'/'RngNative.cs').read_text(encoding='utf-8-sig')
bound=set(re.findall(r'EntryPoint = "(\w+)"',cs))
result={'export_count':len(names),'binding_count':len(bound),'bindings_without_export':sorted(bound-names),'exports_without_binding':sorted(names-bound),'guard_tests':0,'page_guard_tests':0,'alignment_mismatches':[], 'null_constructors':[], 'float_range_failures':[]}
# End the output exactly at an inaccessible page to detect both tail reads
# and writes. VirtualAlloc memory is initialized by Windows.
kernel=C.WinDLL('kernel32',use_last_error=True)
kernel.VirtualAlloc.argtypes=[C.c_void_p,C.c_size_t,C.c_uint32,C.c_uint32];kernel.VirtualAlloc.restype=C.c_void_p
kernel.VirtualProtect.argtypes=[C.c_void_p,C.c_size_t,C.c_uint32,C.POINTER(C.c_uint32)]
kernel.VirtualFree.argtypes=[C.c_void_p,C.c_size_t,C.c_uint32]
page=kernel.VirtualAlloc(None,8192,0x3000,4)
assert page
old=C.c_uint32()
assert kernel.VirtualProtect(page+4096,4096,1,C.byref(old))
seed_types={}
for path in (HERE/'work'/'source'/'src'/'cabi').rglob('*.rs'):
    text=path.read_text(encoding='utf-8')
    seed_types.update(dict(re.findall(r'fn (\w+_new)\(seed: (u32|u64)\)',text)))
for name in sorted(names):
    match=re.fullmatch(r'(\w+)_next_u(32|64)s',name)
    if not match: continue
    stem,bits=match.groups();bits=int(bits)
    new=getattr(lib,stem+'_new');new.restype=C.c_void_p
    new.argtypes=[C.c_uint64 if seed_types.get(stem+'_new')=='u64' or stem.startswith('sfmt') else C.c_uint32]
    free=getattr(lib,stem+'_free');free.argtypes=[C.c_void_p];free.restype=None
    fill=getattr(lib,name);fill.argtypes=[C.c_void_p,C.c_void_p,C.c_size_t];fill.restype=None
    word=C.c_uint32 if bits==32 else C.c_uint64
    sentinel=(1<<bits)-1
    counts=[0,1,3,7,16,17,257,8193,131075]
    if stem in ('splitmix32','pcg32','threefry32x4','sfc32','sfc32x16'):
        counts.append(6291457) # 24 MiB + 4 bytes, exercises the NT path.
    for count in counts:
        outputs=[]
        for shift in [0,1]:
            handle=new(19)
            if not handle:
                result['null_constructors'].append(stem);break
            # Two identical-seed draws, one with a 64-byte aligned output,
            # one offset by a single element but still valid element alignment.
            mem=C.create_string_buffer((count+64)*C.sizeof(word))
            start=(C.addressof(mem)+127)&~63
            start+=shift*C.sizeof(word)
            C.memset(C.addressof(mem),255,C.sizeof(mem))
            before=word.from_address(start-C.sizeof(word));after=word.from_address(start+count*C.sizeof(word))
            fill(handle,start,count)
            assert before.value==sentinel and after.value==sentinel,(name,count,shift,'guard corrupted')
            outputs.append(C.string_at(start,count*C.sizeof(word)))
            free(handle);result['guard_tests']+=1
        if len(outputs)==2 and outputs[0]!=outputs[1]:result['alignment_mismatches'].append([name,count])
    for count in [1,3,15,16,17,63,65]:
        handle=new(19)
        if handle:
            fill(handle,page+4096-count*C.sizeof(word),count)
            free(handle);result['page_guard_tests']+=1
    float_name=stem+('_rand_f32s' if bits==32 else '_rand_f64s')
    if float_name in names:
        f=C.c_float if bits==32 else C.c_double
        draw=getattr(lib,float_name);draw.argtypes=[C.c_void_p,C.c_void_p,C.c_size_t,f,f];draw.restype=None
        handle=new(19)
        if handle:
            buf=(f*1024)();limit=f.from_buffer_copy(bytes.fromhex('ffff7f7f' if bits==32 else 'ffffffffffffef7f')).value
            draw(handle,buf,1024,-limit,limit)
            vals=list(buf)
            if len(set(vals))<=1 or any(not(-limit<=x<limit) for x in vals):
                result['float_range_failures'].append({'function':float_name,'unique':len(set(vals)),'first':vals[:3]})
            free(handle)
assert kernel.VirtualFree(page,0,0x8000)
(HERE/'logs'/'ffi.json').write_text(json.dumps(result,indent=2,allow_nan=True),encoding='utf-8')
print(json.dumps({k:(len(v) if isinstance(v,list) else v) for k,v in result.items()},indent=2))
