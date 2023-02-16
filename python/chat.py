from tknetwork import Network, Peer
from sys import argv

network = Network("0.0.0.0", 5000)


@network.on("connect")
def connection(peer: Peer):
    print(f"\nPeer <{peer.name}> connected\n>> ", end="")

    @peer.on("print")
    def hello_name(data):
        print(f"\n<{peer.name}>: {data}\n>> ", end="")


@network.on("disconnect")
def disconnect(peer: Peer):
    print(f"\nPeer <{peer.name}> disconnected\n>> ", end="")


if __name__ == "__main__":
    if len(argv) == 1:
        network.serve(tcp=False)
    else:
        network.serve(udp=False)

    if len(argv) >= 3 and argv[1] == "-c":
        network.connect(argv[2], 5000)

    while (i := input(">> ")) != "exit":
        network.emit("print", i)
