from socket import *
import threading
import json


class Peer:
    global_events = {}

    def __init__(self, socket: socket, address: str):
        self.socket = socket
        self.address = address
        self.events = {}
        threading.Thread(target=self.__listen, daemon=True).start()
        self.global_events.get("connect", lambda x: None)(self)

    def __str__(self) -> str:
        return f"Peer {self.address}"

    def __listen(self):
        while True:
            try:
                data: bytes = self.socket.recv(1024)
            except:
                self.disconnect()
                return

            if data:
                request = json.loads(data.decode())
                self.__event_handler(request)

    def __event_handler(self, request: dict):
        try:
            if not self.events.get(request["type"], lambda *x: True)(request["data"]):
                return
            self.global_events.get(
                request["type"], lambda *x: None)(request["data"], self)
        finally:
            pass

    def disconnect(self):
        self.global_events.get("disconnect", lambda x: None)(self)
        self.global_events["__disconnect"]({}, self)

    def send(self, data: dict):
        try:
            self.socket.send(json.dumps(data).encode())
        except:
            self.disconnect()

    def emit(self, event: str, data: dict):
        self.send({
            "type": event,
            "data": data
        })

    def on(self, event: str):
        def decorator(func):
            def wrapper(data: dict):
                func(data)
            self.events[event] = func
            return wrapper
        return decorator


class Network:
    def __init__(self, port: int):
        self.port = port
        self.peers = []
        Peer.global_events = {
            "__connect": self.__connection_handler, "__disconnect": self.__disconnect_handler}

    def serve(self, tcp: bool = True):
        if tcp:
            threading.Thread(target=self.__tcp_server, daemon=True).start()
        threading.Thread(target=self.__udp_server, daemon=True).start()

    def __tcp_server(self):
        s = socket(AF_INET, SOCK_STREAM)
        s.bind(("", self.port))
        s.listen()
        while True:
            client, adress = s.accept()
            self.peers.append(Peer(client, adress[0]))

    def __udp_server(self):
        s = socket(AF_INET, SOCK_DGRAM)
        s.bind(("", self.port))
        while True:
            data, (address, _) = s.recvfrom(1024)

            if data == b"connect":
                self.emit("__connect", {"address": address})
                self.__connect(address)

    def __connect(self, address: str):
        peer = socket()
        peer.connect((address, self.port))
        self.peers.append(Peer(peer, address))

    def __connection_handler(self, data: dict, peer: Peer):
        self.__connect(data["address"])

    def __disconnect_handler(self, data: dict, peer: Peer):
        self.peers.remove(peer)
        peer.socket.close()

    def connect(self, address: str):
        s = socket(AF_INET, SOCK_DGRAM)
        s.sendto(b"connect", (address, self.port))

    def emit(self, event: str, data: dict):
        for peer in self.peers:
            peer.emit(event, data)

    @staticmethod
    def on(event: str):
        def decorator(func):
            def wrapper(peer: Peer, data: dict):
                func(data, peer)
            Peer.global_events[event] = func
            return wrapper
        return decorator
