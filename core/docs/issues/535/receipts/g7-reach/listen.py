# throwaway TCP listeners for G7: answer "G7OK <bind>" and log the peer
import socket, threading, sys, time
def serve(host, port):
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((host, port)); s.listen(8); s.settimeout(1.0)
    end = time.time() + float(sys.argv[1])
    while time.time() < end:
        try: c, a = s.accept()
        except socket.timeout: continue
        print(f"{time.strftime('%H:%M:%S')} accept on {host}:{port} from {a}", flush=True)
        c.sendall(f"G7OK {host}:{port}\n".encode()); c.close()
    s.close()
ts = [threading.Thread(target=serve, args=a) for a in [("127.0.0.1", 55442), ("0.0.0.0", 55443)]]
[t.start() for t in ts]; [t.join() for t in ts]
