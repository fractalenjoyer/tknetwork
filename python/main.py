from tknetwork import Network

net = Network()

@net.events.bind("test")
def hello_name(name):
    print(f"Hello, {name}")

net.emit("test", "world")