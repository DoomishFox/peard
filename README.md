# pear 🍐
pear is a simple local network discovery and sharing application.

## broadcast
broadcast is sent to the subnet broadcast address to request client callbacks. These callbacks will include the client's public key which will be used to sign? any shared slices.

## slices
I'm gonna call the things that are shared slices (you know, cause fruit? get it? I'm so funny). slices can be text content, like a url, or maybe even images. We'll see how that will work (I have no idea yet).

## protocol
The pear protocol is pretty simple, and I don't know all of it yet. What I do know is that a message header includes a type, to tell the receiver what type of message it is, and some sender information to identify the sender.

A bare pear data message header looks like this:
```
data message: [0x00] [0x00 0x00 0x00 0x00] { message payload }
              |      |
message type -+      |
sender id -----------+
```

Valid message types are:
- `0x00` - DISC (Discover): discovery broadcast message that requests a callback. This message should only every be sent through UDP.
- `0x01` - DACK (Discover Acknowledge): TCP response to a DISC message. Will include some device information in the payload.
- `0x02` - IREQ (Information Request): TCP message requesting a new DACK response from a specific device.
- `0x10` - SEND (Send): TCP message containing data to be shared with a device.
- `0x11` - SACK (Send Acknowledge): TCP response to a SEND message.