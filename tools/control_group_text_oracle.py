"""Native oracle for the selected-unit control-group text tail."""
from pathlib import Path
import argparse
import hashlib
import json
import struct
u32=lambda x: struct.pack('<I', x & 0xffffffff)
def wide(s): return s.encode('utf-16le')+b'\0\0'
def readwide(e,p):
    chars=[]
    for i in range(128):
        c=struct.unpack('<H',e.uc.mem_read(p+2*i,2))[0]
        if not c: break
        chars.append(chr(c))
    return ''.join(chars)
def dwords(e,p,n=4): return list(struct.unpack('<'+'i'*n,e.uc.mem_read(p,n*4)))

def arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font', required=True, help='Extracted retail GAME.FNT (ra2.mix -> local.mix)')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--write', action='store_true', help='Replace the checked-in native corpus')
    mode.add_argument('--check', action='store_true', help='Compare the native run with the checked-in corpus')
    return parser.parse_args(argv)


def main(argv=None):
    options = arguments(argv)
    # Delay native/oracle dependencies until after argparse handles --help. This
    # keeps importing the tool and asking for usage independent of retail assets
    # and optional native-emulation packages.
    from tools.ai_base_building_oracle import Emu, FAKE, STUBS
    from tools.native_oracle import (image_sha256, run_checked, RET_MAGIC,
                                     STACK_BASE, STACK_SIZE, NATIVE_FPCW)
    from unicorn import UC_HOOK_CODE
    from unicorn.x86_const import (UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EAX,
                                   UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP,
                                   UC_X86_REG_FPCW)
    font_path=Path(options.font)
    raw=font_path.read_bytes()
    if len(raw)!=1584195: raise RuntimeError(f'unexpected retail GAME.FNT length: {len(raw)}')
    font_sha256=hashlib.sha256(raw).hexdigest()
    if font_sha256!='ec8d6d85db3eedf5a862cc6ffb87bfad4c77279dd946bc2d30879ad9140c8d3f':
        raise RuntimeError(f'unexpected retail GAME.FNT SHA-256: {font_sha256}')
    house_color_path=Path(__file__).with_name('procedural_drawing_oracle')/'house_color.json'
    house_color_corpus=json.loads(house_color_path.read_text(encoding='utf-8'))
    gold_rows=[row for row in house_color_corpus['colors'] if row.get('name')=='Gold']
    if len(gold_rows)!=1: raise RuntimeError(f'expected one Gold entry in {house_color_path}')
    house_rgb_input=gold_rows[0]['modes']['scalar']['rgb']
    m=Emu();u=m.uc
    renderer=FAKE+0x20000; obj=FAKE+0x28000; vt=FAKE+0x29000; house=FAKE+0x30000
    surface=FAKE+0x31000;svt=FAKE+0x32000;anchor=FAKE+0x33000;clip=FAKE+0x33100
    FILE_VT=FAKE+0x24000
    FILE_STUBS={'close':STUBS+0x200,'open':STUBS+0x204,'size':STUBS+0x208,
                'read':STUBS+0x20c,'seek':STUBS+0x210,'lock':STUBS+0x214}
    for off,name in [(0x00,'close'),(0x14,'open'),(0x1c,'size'),(0x24,'read'),(0x28,'seek')]:
        m.write32(FILE_VT+off,FILE_STUBS[name])
    loader={'position':0,'reads':[],'seeks':[],'path':None,'open_modes':[],'close_flags':[]}
    def readnarrow(e,p):
        out=bytearray()
        for i in range(128):
            b=e.uc.mem_read(p+i,1)[0]
            if b==0: break
            out.append(b)
        return out.decode('ascii')
    def file_ctor(e):
        this=e.uc.reg_read(UC_X86_REG_ECX); pathptr=e.arg(0)
        try: path=readnarrow(e,pathptr)
        except Exception as err:
            raise RuntimeError(f'font file constructor this=0x{this:08x} pathptr=0x{pathptr:08x} sp=0x{e.uc.reg_read(UC_X86_REG_ESP):08x}: {err}')
        if path.upper()!='GAME.FNT': raise RuntimeError(f'unexpected native font resource {path!r}')
        loader['path']=path;loader['position']=0;e.write32(this,FILE_VT);return this
    def file_close(e):
        loader['close_flags'].append(e.arg(0));return 1
    def file_open(e):
        mode=e.arg(0);loader['open_modes'].append(mode);loader['position']=0;return 1
    def file_size(e): return len(raw)
    def file_read(e):
        dest,count=e.arg(0),e.arg(1);start=loader['position'];end=start+count
        if end>len(raw): raise RuntimeError(f'native read past GAME.FNT: offset=0x{start:x} size=0x{count:x}')
        e.uc.mem_write(dest,raw[start:end]);loader['position']=end
        loader['reads'].append({'file_offset':start,'size':count,'destination':hex(dest)})
        return count
    def file_seek(e):
        offset=struct.unpack('<i',u32(e.arg(0)))[0];origin=e.arg(1)
        newpos=offset if origin==0 else loader['position']+offset if origin==1 else len(raw)+offset
        if not 0<=newpos<=len(raw): raise RuntimeError(f'invalid native font seek offset={offset} origin={origin}')
        loader['seeks'].append({'offset':offset,'origin':origin,'position':newpos});loader['position']=newpos;return newpos
    for address,callback,pops in [
        (0x4739f0,file_ctor,4),(FILE_STUBS['close'],file_close,4),
        (FILE_STUBS['open'],file_open,4),(FILE_STUBS['size'],file_size,4),
        (FILE_STUBS['read'],file_read,8),(FILE_STUBS['seek'],file_seek,8),
        (FILE_STUBS['lock'],lambda _e:None,0x14),
        (0x7c9430,lambda e:e.operator_new(e),0),(0x7c93e8,lambda _e:None,0)]:
        m.hook(address,callback,pops)
    m.write32(0x7e14b4,FILE_STUBS['lock'])
    renderer=m.invoke(0x434ad0,ecx=renderer,
        required=(0x433880,0x433990,0x433a3f,0x433a46,0x433b84,0x433bba,0x433bbf,0x433bee,0x434700))
    # The actual caller at 0x52BB95 stores this constructor result in the renderer global.
    m.write32(0x89c4b8,renderer)
    font=m.read32(0x89c4d0);data=m.read32(font+4)
    font_meta={'font_object':hex(font),'renderer_object':hex(renderer),'font_data':hex(data),
        'height':m.read32(font+0x1c),'bytes_per_row':m.read32(data+4),'bitmap_rows':m.read32(data+8),
        'glyph_slots':m.read32(data+0x10),'glyph_stride':m.read32(data+0x14),
        'lookup':hex(m.read32(data+0x18)),'glyphs':hex(m.read32(data+0x1c))}
    lookup=m.read32(data+0x18);glyphs=m.read32(data+0x1c);stride=m.read32(data+0x14)
    if u.mem_read(lookup,0x20000)!=raw[0x1c:0x2001c]: raise RuntimeError('original loader lookup bytes differ from retail GAME.FNT file range')
    glyph_bytes=m.read32(data+0x10)*stride
    if u.mem_read(glyphs,glyph_bytes)!=raw[0x2001c:0x2001c+glyph_bytes]: raise RuntimeError('original loader glyph bytes differ from retail GAME.FNT file range')
    expected_reads=[(0,0x1c),(0x1c,0x20000),(0x2001c,0x162c27)]
    actual_reads=[(r['file_offset'],r['size']) for r in loader['reads']]
    if actual_reads!=expected_reads or loader['position']!=len(raw):
        raise RuntimeError(f'original loader read ranges differ: {actual_reads}, final={loader["position"]}, length={len(raw)}')
    native_digit_metrics=[]
    for digit in range(10):
        code=0x30+digit;index=struct.unpack('<H',u.mem_read(lookup+code*2,2))[0]
        glyph=glyphs+(index-1)*stride if index else 0;width=u.mem_read(glyph,1)[0] if glyph else 0
        native_digit_metrics.append({'digit':digit,'lookup_index':index,'glyph_width':width,'advance':width+m.read32(font+0x2c)})
    if loader['path']!='GAME.FNT' or loader['reads'][-1]['file_offset']!=0x2001c:
        raise RuntimeError(f'original loader did not read the retail glyph block from +0x2001c: {loader}')
    for a,v in [(obj,vt),(obj+0x21c,house),
     (vt+0x2c,STUBS+0x100),(0x887314,surface),(surface,svt),(svt+0x14,STUBS+0x110),(svt+0x58,STUBS+0x120)]: m.write32(a,v)
    # Feed stock Gold's native House+0x56F9 result through the original text tail.
    u.mem_write(house+0x56f9,bytes(house_rgb_input))
    # The actual renderer format is an input to this bounded presentation fixture: RGB565.
    u.mem_write(0x8a0dd4,b'\x03');m.write32(0x8a0dd0,11)
    u.mem_write(0x8a0de4,b'\x02');m.write32(0x8a0de0,5)
    u.mem_write(0x8a0ddc,b'\x03');m.write32(0x8a0dd8,0)
    state={'kind':1,'events':[],'frame':0}
    def kind_stub(e): return state['kind']
    def swprintf(e):
        dest,fmt,value=e.arg(0),e.arg(1),e.arg(2); form=readwide(e,fmt)
        value=struct.unpack('<i',struct.pack('<I',value))[0]
        if form!='%d': raise RuntimeError(f'unexpected native format {form!r}')
        rendered=str(value);e.uc.mem_write(dest,wide(rendered));state['events'].append(['format',value,rendered]);return len(rendered)
    def fill(e):
        p=e.arg(0);state['events'].append(['fill',dwords(e,p),e.arg(1)]);return 1
    def surface_draw(e):
        p=e.arg(0);state['events'].append(['surface_draw',dwords(e,p),e.arg(1)]);return 1
    def text_consumer(e):
        args=[e.arg(i) for i in range(8)];ecx=e.uc.reg_read(UC_X86_REG_ECX);edx=e.uc.reg_read(UC_X86_REG_EDX)
        row={'text':readwide(e,edx),'text_ptr':hex(edx),'ecx_this':hex(ecx),'stack_args':[hex(v) for v in args],
             'clip':dwords(e,args[1]),'arg2_data':dwords(e,args[2]),'packed_owner_rgb':hex(args[3]),
             'arg4':args[4],'flags':hex(args[5]),'color_mode':args[6],'draw_mode':args[7]}
        state['events'].append(['text_draw_consumer',row]);return 0
    def after_measure(uc,address,size,userdata):
        if address==0x70a77b:
            p=uc.reg_read(UC_X86_REG_EAX);state['events'].append(['measure_rect',dwords(type('E',(),{'uc':uc})(),p)])
    def after_font_measure(uc,address,size,userdata):
        if address==0x4a59f6:
            state['events'].append(['font_measure',uc.reg_read(UC_X86_REG_EAX)])
    # Existing fixture owns only these non-game edges: format CRT and renderer submission / virtual surface.
    m.hook(STUBS+0x100,kind_stub,0);m.hook(0x7ca564,swprintf,0)
    m.hook(STUBS+0x110,fill,8);m.hook(STUBS+0x120,surface_draw,8);m.hook(0x4a66d0,text_consumer,0x20)
    u.hook_add(UC_HOOK_CODE,after_measure);u.hook_add(UC_HOOK_CODE,after_font_measure)
    cases=[{'slot':i,'kind':1,'clip':[0,0,640,480],'label':'full','caller_point':[320,200]} for i in (0,9,1,8)]
    cases += [{'slot':0,'kind':1,'clip':[305,177,312,185],'label':'partial','caller_point':[320,200]},
              {'slot':0,'kind':1,'clip':[0,0,100,100],'label':'outside','caller_point':[320,200]},
              {'slot':0,'kind':0xF,'clip':[0,0,640,480],'label':'infantry','caller_point':[320,200]}]
    # Aircraft WhatAmI 2 takes the same non-building caller and shared tail as
    # units; keep both registered endpoint digits explicit in the native corpus.
    cases += [
        {'slot':0,'kind':2,'clip':[0,0,640,480],'label':'aircraft_1','caller_point':[320,200]},
        {'slot':9,'kind':2,'clip':[0,0,640,480],'label':'aircraft_0','caller_point':[320,200]},
    ]
    # These two caller points come from the aligned native DrawHealthBar
    # Building arm at 0x6F64A0. Its BuildingTypeClass::Dimension2 slot at
    # 0x464AF0 returns the retail dimensions below; the original projection
    # helper at 0x6D1F10 produced the recorded half-foundation offsets before
    # the shared 0x709A90 tail. The tail itself still runs natively here.
    cases += [
        {'slot':0,'kind':6,'clip':[0,0,640,480],'label':'building_2x2',
         'caller_point':[320,200],'tail_input_point':[260,200],
         'type_name':'YAPOWR','foundation':'2x2',
         'native_dimension2':[512,512,416],
         'native_projection_input':[-256,256,0],
         'native_projected_offset':[-60,0]},
        {'slot':9,'kind':6,'clip':[0,0,640,480],'label':'building_4x4',
         'caller_point':[320,200],'tail_input_point':[200,200],
         'type_name':'GACNST','foundation':'4x4',
         'native_dimension2':[1024,1024,416],
         'native_projection_input':[-512,512,0],
         'native_projected_offset':[-120,0]},
    ]
    rows=[]
    for case in cases:
        state['kind']=case['kind'];state['events']=[];m.write32(obj+0x214,case['slot'])
        # The regular non-building caller, including Aircraft, supplies
        # caller_point + {-10,+10}.
        # The native Building arm supplies its projected foundation point
        # directly, captured separately as tail_input_point.
        caller_x,caller_y=case['caller_point']
        tail_x,tail_y=case.get('tail_input_point',(caller_x-10,caller_y+10))
        u.mem_write(anchor,struct.pack('<ii',tail_x,tail_y));u.mem_write(clip,struct.pack('<4i',*case['clip']))
        frame=STACK_BASE+STACK_SIZE-0x10000;state['frame']=frame
        u.mem_write(frame,struct.pack('<4I',0x11111111,0x22222222,0x33333333,0x44444444)+bytes(0x60))
        u.mem_write(frame+0x74,u32(RET_MAGIC));u.mem_write(frame+0x78,u32(anchor));u.mem_write(frame+0x7c,u32(0));u.mem_write(frame+0x80,u32(clip))
        u.reg_write(UC_X86_REG_ESP,frame);u.reg_write(UC_X86_REG_EBP,obj);u.reg_write(UC_X86_REG_EBX,anchor);u.reg_write(UC_X86_REG_FPCW,NATIVE_FPCW)
        run_checked(u,0x70a6fd,RET_MAGIC,count=500000,required_addresses=(0x70a776,0x70a97b),
          context={'slot':case['slot'],'kind':case['kind'],'clip':case['clip'],'fixture':'tail 70A6FD with original font measure/clipping; final renderer submission stub'})
        rows.append({'case':case,'events':state['events']})
    def event(row,name): return next(e for e in row['events'] if e[0]==name)
    corpus_rows=[]
    for row in rows:
        if row['case']['label']=='full' and row['case']['slot'] not in (0,9,1,8): continue
        fmt=event(row,'format');measure=event(row,'font_measure');measure_rect=event(row,'measure_rect')
        fill_event=event(row,'fill');surface_event=event(row,'surface_draw');submit=event(row,'text_draw_consumer')[1]
        corpus_rows.append({
            'case':row['case'],'text':fmt[2],'font_measure':measure[1],
            'measure_rect':measure_rect[1],
            'background_fill':{'rect':fill_event[1],'color_arg':fill_event[2]},
            'surface_draw':{'rect':surface_event[1],'color_arg':surface_event[2]},
            'text_submission':{
                'clip':submit['clip'],'position':submit['arg2_data'][:2],
                'packed_owner_rgb':submit['packed_owner_rgb'],'arg4':submit['arg4'],
                'flags':submit['flags'],'color_mode':submit['color_mode'],'draw_mode':submit['draw_mode']}})
    payload={
        'schema':'control-group-tail-native-v1',
        'provenance':{
            'gamemd_sha256':image_sha256(),
            'font':{'name':'GAME.FNT','resolution':'ra2.mix -> local.mix',
                    'file_length':len(raw),'file_length_hex':hex(len(raw)),
                    'sha256':font_sha256},
            'house_rgb_input':house_rgb_input,
            'house_rgb_input_source':'tools/procedural_drawing_oracle/house_color.json: Gold scalar House+0x56F9 RGB',
            'command':"$env:VERA20K_GAMEMD_EXE='<supported retail gamemd.exe>'; python -m tools.control_group_text_oracle --font '<extracted retail GAME.FNT>' --write",
            'substitutions':['native virtual file I/O returns the pinned retail GAME.FNT bytes','native allocator and lock edges are stubbed','CRT %d formatting is stubbed','virtual fill/surface and final 4A66D0 text renderer edges are submission captures/stubs'],
            'coverage_limit':'Original loader, font measurement, tail rect and clip path execute. Final renderer submission arguments are captured, but glyph raster output is not established.'},
        'native_font_initialization':{
            'native_chain':'434AD0 -> 433880 -> 433990 -> 434700; caller 52BB95 stores returned renderer at global 89C4B8',
            'renderer_global':'0x89c4b8','current_font_global':'0x89c4d0','resource':loader['path'],
            'native_file_reads':[{'file_offset':o,'file_offset_hex':hex(o),'size':n,'size_hex':hex(n)} for o,n in expected_reads],
            'runtime_fields':{'font_height':font_meta['height'],'font_bytes_per_row':font_meta['bytes_per_row'],
                'bitmap_rows':font_meta['bitmap_rows'],'glyph_slots':font_meta['glyph_slots'],
                'glyph_stride':font_meta['glyph_stride'],'char_spacing':m.read32(font+0x2c)},
            'digit_metrics':native_digit_metrics},
        'cases':corpus_rows}
    out=Path(__file__).with_name('control_group_text_native.json')
    rendered=json.dumps(payload,separators=(',',':'))+'\n'
    if options.write:
        out.write_text(rendered,encoding='utf-8')
    else:
        if not out.is_file():
            raise RuntimeError(f'missing native corpus: {out}')
        expected=out.read_text(encoding='utf-8')
        if rendered != expected:
            raise RuntimeError(f'native output differs from {out}; inspect before replacing it with --write')
    print(json.dumps({'corpus':str(out),'cases':len(corpus_rows),'native_sha256':payload['provenance']['gamemd_sha256'],
        'font_sha256':payload['provenance']['font']['sha256'],'native_file_reads':payload['native_font_initialization']['native_file_reads'],
        'digit_advances':[x['advance'] for x in native_digit_metrics]},separators=(',',':')))




if __name__ == '__main__':
    main()
