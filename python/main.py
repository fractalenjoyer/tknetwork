import tknetwork
# from concurrent.futures import ThreadPoolExecutor

def on_message(message):
    print(message)

# executor = ThreadPoolExecutor(max_workers=1)
# executor.submit(tknetwork.start_server, ("127.0.0.1:12345", on_message))

# threading.Thread(target=tknetwork.start_server, args=(), daemon=True)
tknetwork.udp_server("127.0.0.1:12345", on_message)
while (i:=input()) != "exit":
    tknetwork.udp_send(i, "127.0.0.1:12345")
