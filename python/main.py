from tknetwork import Network

net = Network("0.0.0.0", 5000)

@net.events.bind("connect")
def connect(address):
    net.tcp_connect(address, 5000)

@net.events.bind("print")
def hello_name(data):
    print(data)

# net.tcp_server()
net.udp_server()

while (i := input(">> ")) != "exit":
    net.emit("print", i)