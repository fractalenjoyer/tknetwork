from tknetwork import Network, Peer
from sys import argv

network = Network("0.0.0.0", 5000)


@network.events.bind("new_peer")
def connection(peer: Peer):
    print(f"\nPeer <{peer.address}> connected\n>> ", end="")

    @peer.events.bind("print")
    def hello_name(data):
        print(f"\n<{peer.address}>: {data}\n>> ", end="")


@network.events.bind("peer_disconnect")
def disconnect(peer: Peer):
    print(f"\nPeer <{peer.address}> disconnected\n>> ", end="")


@network.events.bind("connect")
def connect(address):
    network.tcp_connect(address, 5000)


if __name__ == "__main__":
    if len(argv) == 1:
        network.udp_server()

    elif len(argv) >= 3 and argv[1] == "-c":
        network.tcp_server()
        network.connect(argv[2], 5000)

    while (i := input(">> ")) != "exit":
        network.emit("print", i)
