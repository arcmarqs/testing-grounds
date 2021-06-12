package febft.ycsb;

// TLS in Java:
// https://blog.gypsyengineer.com/en/security/an-example-of-tls-13-client-and-server-on-java.html

import java.util.Random;
import java.util.Map;
import java.util.HashMap;
import java.util.List;
import java.util.Arrays;
import java.util.ArrayList;
import java.nio.ByteBuffer;
import java.io.IOException;
import java.io.DataInputStream;
import java.io.OutputStream;
import java.util.concurrent.Callable;
import static java.nio.ByteOrder.LITTLE_ENDIAN;

import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLServerSocket;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SNIServerName;
import javax.net.ssl.SNIHostName;

import site.ycsb.Status;
import site.ycsb.ByteIterator;

import static febft.ycsb.Config.Entry;
import febft.ycsb.SystemMessage;
import febft.ycsb.RequestMessage;
import febft.ycsb.ReplyMessage;
import febft.ycsb.Config;
import febft.ycsb.IdCounter;
import febft.ycsb.Update;
import febft.ycsb.Pool;

public class Node {
    private static final Object LOG_MUX = new Object();

    private static final int BUF_CAP = 1024 * 1024; // 1 MiB

    private static final String[] PROTOCOLS = {"TLSv1.3"};
    private static final String[] CIPHER_SUITES = {"TLS_AES_128_GCM_SHA256"};

    private Entry config;
    private int noReplicas;
    private SSLServerSocket listener = null;
    private Map<Integer, OutputStream> tx;
    private Map<Integer, DataInputStream> rx;
    private Random rng;

    public Node() {
        config = Config.getClients().get(new Integer(IdCounter.nextId()));
        rng = new Random();
    }

    public void close() {
        // NOTE: only close the listener for now,
        // since we haven't implemented handling
        // disconnected clients in `febft` yet
        if (listener != null) {
            try {
                listener.close();
            } catch (IOException e) {
                // noop
            } finally {
                listener = null;
            }
        }
    }

    public void bootstrap() throws IOException {
        listener = listen(config.getId(), config.getHostname(), config.getPortNo());

        noReplicas = 0;
        final Map<Integer, Entry> replicas = Config.getReplicas();

        this.tx = new HashMap<>();
        this.rx = new HashMap<>();

        ByteBuffer txBuf = ByteBuffer.allocate(BUF_CAP).order(LITTLE_ENDIAN);

        // connect to replicas
        for (Entry replicaConfig : replicas.values()) {
            printf("Connecting to node %d\n", replicaConfig.getId());
            OutputStream writer = connect(
                config.getId(),
                replicaConfig.getHostname(),
                replicaConfig.getIpAddr(),
                replicaConfig.getPortNo()
            );
            printf("Connected to node %d\n", replicaConfig.getId());

            Header header = new Header(
                config.getId(),
                replicaConfig.getId(),
                rng.nextLong(),
                null
            );

            printf("Writing header to node %d\n", replicaConfig.getId());
            txBuf.clear();
            header.serializeInto(txBuf);
            writer.write(txBuf.array(), 0, txBuf.position());
            writer.flush();
            printf("Written: %s\n", header);

            tx.put(replicaConfig.getId(), writer);
            noReplicas++;
        }

        // accept conns from replicas
        for (int i = 0; i < noReplicas; i++) {
            printf("Accepting connection no. %d\n", i);
            SSLSocket socket = (SSLSocket)listener.accept();
            DataInputStream reader = new DataInputStream(socket.getInputStream());
            println("Accepted, reading header");

            txBuf.clear();
            reader.readFully(txBuf.array(), 0, Header.LENGTH);
            txBuf.limit(Header.LENGTH);

            Header header = Header.deserializeFrom(txBuf);
            printf("Read header: %s\n", header);

            rx.put(header.getFrom(), reader);
        }
    }

    public Status callService(Update... updates) throws IOException {
        List<Callable<Status>> callables = new ArrayList<>(noReplicas);
        for (int i = 0; i < noReplicas; i++) {
            final int nodeId = i;
            final DataInputStream input = rx.get(nodeId);
            final OutputStream output = tx.get(nodeId);
            callables.add(() -> {
                ByteBuffer requestBuf = (new RequestMessage(updates)).serialize();
                ByteBuffer headerBuf = ByteBuffer.allocate(Header.LENGTH).order(LITTLE_ENDIAN);
                printf("Serialized request (len=%d)\n", requestBuf.position());

                Header header = new Header(
                    config.getId(),
                    nodeId,
                    nextNonce(),
                    requestBuf.array(),
                    requestBuf.position()
                );
                header.serializeInto(headerBuf);
                byte[] requestDigest = header.getDigest();

                output.write(headerBuf.array(), 0, headerBuf.position());
                output.write(requestBuf.array(), 0, requestBuf.position());
                output.flush();
                printf("Sent header and request pair over the wire: %s\n", header);

                headerBuf.clear();
                input.readFully(headerBuf.array(), 0, Header.LENGTH);
                headerBuf.limit(Header.LENGTH);
                header = Header.deserializeFrom(headerBuf);
                printf("Read and deserialized header from the wire: from %d\n", header.getFrom());

                ByteBuffer payloadBuf = ByteBuffer.allocate((int)header.getLength());
                input.readFully(payloadBuf.array(), 0, (int)header.getLength());
                ReplyMessage reply = (ReplyMessage)SystemMessage.deserializeAs(ReplyMessage.class, payloadBuf);
                println("Read and deserialized payload from the wire");

                if (reply == null) {
                    return Status.ERROR;
                }
                assert Arrays.equals(requestDigest, reply.getDigest());

                return reply.getStatus();
            });
            printf("Added callable to node %d\n", i);
        }
        return Pool.call(callables);
    }

    private synchronized long nextNonce() {
        return rng.nextLong();
    }

    public void printf(String f, Object... args) {
        synchronized (LOG_MUX) {
            System.err.printf(
                (new StringBuilder()).append(config.getId()).append(": ").append(f).toString(),
                args
            );
        }
    }

    public void println(String s) {
        synchronized (LOG_MUX) {
            System.err.println(
                (new StringBuilder()).append(config.getId()).append(": ").append(s).toString()
            );
        }
    }

    public Entry getConfig() {
        return config;
    }
    
    
    private static OutputStream connect(int id, String sni, String host, int port) throws IOException {
        SSLSocket socket = (SSLSocket)
            Config.getSslSocketFactory(id).createSocket(host, port);

        SSLParameters params = socket.getSSLParameters();
        List<SNIServerName> serverNames = Arrays.asList(new SNIHostName(sni));

        params.setProtocols(PROTOCOLS);
        params.setCipherSuites(CIPHER_SUITES);
        params.setServerNames(serverNames);

        socket.setSSLParameters(params);
        return socket.getOutputStream();
    }
    
    private static SSLServerSocket listen(int id, String sni, int port) throws IOException {
        SSLServerSocket socket = (SSLServerSocket)
            Config.getSslServerSocketFactory(id).createServerSocket(port);

        SSLParameters params = socket.getSSLParameters();
        List<SNIServerName> serverNames = Arrays.asList(new SNIHostName(sni));

        params.setProtocols(PROTOCOLS);
        params.setCipherSuites(CIPHER_SUITES);
        params.setServerNames(serverNames);

        socket.setSSLParameters(params);
        return socket;
    }
}
