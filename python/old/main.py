from p2p import Network, Peer
from sys import argv

network = Network(7337)


@network.on("draw")
def draw(data: dict):
    print(data)


# @network.on("message")
# def message(data: dict, peer):
#     print(f"{peer}: {data['message']}")


@network.on("connect")
def connect(peer: Peer):
    print(f"{peer} connected")

    @peer.on("message")
    def message(data: dict):
        print(f"{peer}: {data['message']}")


@network.on("disconnect")
def disconnect(peer: Peer):
    print(f"{peer} disconnected")


if __name__ == "__main__":
    if len(argv) == 1:
        network.serve(False)
        
    elif len(argv) >= 3 and argv[1] == "-c":
        network.serve()
        network.connect(argv[2])
        
    while (i := input()) != "exit":
        network.emit("message", {
            "message": i
        })
