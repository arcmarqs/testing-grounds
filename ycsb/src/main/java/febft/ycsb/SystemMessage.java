package febft.ycsb;

import java.nio.ByteBuffer;
import java.io.IOException;

import febft.ycsb.MessageKind;

public abstract class SystemMessage {
    public ByteBuffer serialize() {
        throw new UnsupportedOperationException();
    }

    protected SystemMessage deserialize(ByteBuffer buf) throws IOException {
        throw new UnsupportedOperationException();
    }

    public static <T extends SystemMessage> SystemMessage deserializeAs(Class<T> kls, ByteBuffer buf) {
        try {
            return kls.newInstance().deserialize(buf);
        } catch (Exception e) {
            return null;
        }
    }

    public abstract MessageKind getKind();
}
