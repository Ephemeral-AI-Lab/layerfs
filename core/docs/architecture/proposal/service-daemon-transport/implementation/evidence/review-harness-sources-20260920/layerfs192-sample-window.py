import subprocess,time
from pathlib import Path
binary=Path('core/target/debug/deps/metadata_window-be87e160374e3c42')
process=subprocess.Popen([binary,'crossing_the_window','--nocapture'])
try:
 time.sleep(.5)
 result=subprocess.run(['sample',str(process.pid),'2','10','-file','/tmp/layerfs192-window-sample.txt'],capture_output=True,text=True,timeout=5)
 print(result.stdout,result.stderr)
 code=process.wait(timeout=20)
 sample=Path('/tmp/layerfs192-window-sample.txt').read_text();print(sample[:24000]);assert code==0
finally:
 if process.poll() is None:process.kill();process.wait()
