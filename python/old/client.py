from tknetwork import Network, Peer

net = Network("0.0.0.0", 5000)

@net.events.bind("connect")
def connect(address):
    net.tcp_connect(address, 5000)

@net.events.bind("new_peer")
def connection(peer):
    print(f"\nPeer <{peer.address}> connected\n>> ", end="")
    @peer.events.bind("print")
    def hello_name(data):
        print(f"\n<{peer.address}>: {data}\n>> ", end="")
        
@net.events.bind("peer_disconnect")
def disconnect(peer):
    print(f"\nPeer <{peer.address}> disconnected\n>> ", end="")

@net.events.bind("print")
def hello_name(data):
    print(data)

net.tcp_server()
# net.udp_server()
net.connect("127.0.0.1", 5000)

while (i := input(">> ")) != "exit":
    net.emit("print", i)