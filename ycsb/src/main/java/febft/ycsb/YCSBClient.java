package febft.ycsb;

import java.util.*;
import java.io.IOException;
import java.security.Security;

//import org.bouncycastle.jce.provider.BouncyCastleProvider;

import febft.ycsb.Node;

import site.ycsb.ByteIterator;
import site.ycsb.Status;
import site.ycsb.DB;

public class YCSBClient extends DB {
    private Node node;

    public YCSBClient() {
        // empty constructor
    }

    // test
    public static void main(String[] args) throws Exception {
        YCSBClient client = new YCSBClient();
        System.err.println("Initializing...");
        client.init();
        System.err.println("Done.");
        Thread.sleep(60 * 60 * 1000);
    }

    @Override
    public void init() {
        //Security.addProvider(new BouncyCastleProvider());

        System.setProperty("javax.net.ssl.keyStore", "ca-root/keystore.jks");
        System.setProperty("javax.net.ssl.keyStorePassword", "123456");

        System.setProperty("javax.net.ssl.trustStore", "ca-root/truststore.jks");
        System.setProperty("javax.net.ssl.trustStorePassword", "123456");

        this.node = new Node();
        int id = this.node.getConfig().getId();

        try {
            node.bootstrap();
        } catch (IOException e) {
            System.err.printf("Failed to bootstrap node %d: %s\n", id, e);
            System.exit(1);
        }
    }

    @Override
    public Status update(String table, String key, Map<String, ByteIterator> values) {
        // TODO: implement update
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status read(String table, String key, Set<String> fields, Map<String, ByteIterator> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status scan(String table, String startkey, int recordcount, Set<String> fields,
                       Vector<HashMap<String, ByteIterator>> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status insert(String table, String key, Map<String, ByteIterator> values) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status delete(String table, String key) {
        return Status.NOT_IMPLEMENTED;
    }
}
