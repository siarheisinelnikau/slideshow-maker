"""Generate original test tones; optionally encode with an existing local libVLC.

No player interface, audio output, downloads, installation or user media. The
library is used only as an independent file encoder, never by SlideshowMaker.
Run this generator under an external timeout (60 seconds is sufficient).
"""
import argparse
import ctypes as c
import math
import os
from pathlib import Path
import struct
import time
import wave

parser = argparse.ArgumentParser(__doc__)
parser.add_argument('--vlc-dir', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
output = root / 'tests' / 'fixtures' / 'audio'
output.mkdir(parents=True, exist_ok=True)
rate = 44100
pcm = b''.join(struct.pack('<hh', round(12000 * math.sin(2 * math.pi * 440 * n / rate)),
                          round(9000 * math.sin(2 * math.pi * 660 * n / rate))) for n in range(rate))
source = output / 'tone.wav'
with wave.open(str(source), 'wb') as file:
    file.setparams((2, 2, rate, rate, 'NONE', 'not compressed'))
    file.writeframes(pcm)
comm = struct.pack('>hIh', 2, rate, 16) + bytes.fromhex('400eac44000000000000')
big_pcm = b''.join(struct.pack('>h', value[0]) for value in struct.iter_unpack('<h', pcm))
chunks = b'COMM' + struct.pack('>I', len(comm)) + comm + b'SSND' + struct.pack('>III', len(big_pcm) + 8, 0, 0) + big_pcm
(output / 'tone.aiff').write_bytes(b'FORM' + struct.pack('>I', len(chunks) + 4) + b'AIFF' + chunks)

# Keep all library loading scoped to this encoder process.
dll_directory = os.add_dll_directory(str(args.vlc_dir.resolve()))
vlc = c.CDLL(str(args.vlc_dir.resolve() / 'libvlc.dll'))
signatures = {
    'libvlc_new': (c.c_void_p, [c.c_int, c.POINTER(c.c_char_p)]),
    'libvlc_release': (None, [c.c_void_p]),
    'libvlc_get_version': (c.c_char_p, []),
    'libvlc_media_new_path': (c.c_void_p, [c.c_void_p, c.c_char_p]),
    'libvlc_media_add_option': (None, [c.c_void_p, c.c_char_p]),
    'libvlc_media_release': (None, [c.c_void_p]),
    'libvlc_media_player_new_from_media': (c.c_void_p, [c.c_void_p]),
    'libvlc_media_player_play': (c.c_int, [c.c_void_p]),
    'libvlc_media_player_get_state': (c.c_int, [c.c_void_p]),
    'libvlc_media_player_stop': (None, [c.c_void_p]),
    'libvlc_media_player_release': (None, [c.c_void_p]),
}
for name, (result, parameters) in signatures.items():
    function = getattr(vlc, name)
    function.restype = result
    function.argtypes = parameters
options = [b'--ignore-config', b'--no-plugins-cache', b'--no-video', b'--aout=dummy',
           b'--vout=dummy', b'--no-metadata-network-access', b'--no-lua', b'--quiet']
instance = vlc.libvlc_new(len(options), (c.c_char_p * len(options))(*options))
if not instance:
    raise RuntimeError('Cannot create isolated libVLC encoder')
print('Encoder:', vlc.libvlc_get_version().decode(), flush=True)
try:
    for name, codec, mux in [('tone.mp3', 'mp3', 'raw'), ('tone.flac', 'flac', 'raw'),
                             ('tone.ogg', 'vorb', 'ogg'), ('tone.m4a', 'mp4a', 'mp4'),
                             ('tone.aac', 'mp4a', 'avformat{mux=adts}')]:
        target = output / name
        if target.exists():
            print('Keeping existing fixture:', name, flush=True)
            continue
        media = vlc.libvlc_media_new_path(instance, str(source).encode())
        destination = target.as_posix()
        chain = f':sout=#transcode{{acodec={codec},ab=128,channels=2,samplerate={rate}}}:std{{access=file,mux={mux},dst="{destination}"}}'
        vlc.libvlc_media_add_option(media, chain.encode())
        vlc.libvlc_media_add_option(media, b':no-sout-all')
        vlc.libvlc_media_add_option(media, b':no-sout-keep')
        player = vlc.libvlc_media_player_new_from_media(media)
        try:
            if vlc.libvlc_media_player_play(player) != 0:
                raise RuntimeError(f'Cannot start {name} encoder')
            deadline = time.monotonic() + 8
            while (state := vlc.libvlc_media_player_get_state(player)) not in (6, 7):
                if time.monotonic() > deadline:
                    raise TimeoutError(name)
                time.sleep(0.025)
            if state == 7:
                raise RuntimeError(f'Encoder failed for {name}')
        finally:
            vlc.libvlc_media_player_stop(player)
            vlc.libvlc_media_player_release(player)
            vlc.libvlc_media_release(media)
        if not target.exists() or target.stat().st_size < 1000:
            raise RuntimeError(f'Encoder produced no {name} data')
        print(name, target.stat().st_size, flush=True)
finally:
    vlc.libvlc_release(instance)
    dll_directory.close()
