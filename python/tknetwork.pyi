class Event:
    def __call__(func: function) -> function: ...


class Peer:
    name: str

    def on(self, event: str) -> Event:
        """
        Decorator to register a function to an event.

        Parameters:
            event (str): Name of the event to register to.
        """
        ...

    def emit(self, event: str, data: str):
        """
        Emit an event to a peer.

        Parameters:
            event (str): Name of the event to emit.
            data (str): Data to send to the peer.
        """
        ...


class Network:
    """
    Class to represent a peer-to-peer network.

    IP should in most cases be 0.0.0.0, which means that the network will be available on all interfaces. Port should be a number between 1024 and 65535.

    A special event should be registered with @net.on("connect") to handle new connections. The function should take a single parameter, which is the peer that connected.
    Optionally, an event can be registered with @net.on("disconnect") to handle disconnects. The function should take a single parameter, which is the peer that disconnected.

    Parameters:
        ip (str): IP address of the network.
        port (int): Port of the network.
    """
    def __init__(ip: str, port: int): ...

    def connect(self, ip: str, port: int):
        """
        Connect to a peer-to-peer network.

        Parameters:
            ip (str): IP address of a peer in the network.
            port (int): Port of the respective peer.
        """
        ...

    def on(self, event: str) -> Event:
        """
        Decorator to register a function to global events.

        Parameters:
            event (str): Name of the event to register to.
        """
        ...

    def emit(self, event: str, data: str):
        """
        Emit an event to all peers.

        Parameters:
            event (str): Name of the event to emit.
            data (str): Data to send to all peers.
        """
        ...

    def serve(self, tcp=True, udp=True):
        """
        Serve as a peer-to-peer network.

        Both TCP and UDP is required to support a functioning peer-to-peer network. But for testing purposes, it is possible to serve only one of them.

        Parameters:
            tcp (bool): Whether to serve TCP connections required to connect.
            udp (bool): Whether to serve UDP connections required to allow connections.
        """
        ...
