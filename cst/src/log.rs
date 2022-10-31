use std::collections::HashMap;

/* 
Simplified log implementation to test state transfers 
*/
use crate::node::Operation;

pub struct Checkpoint 
{
    checkpoint_seq: u32,
    last_exec_op: u32,
    app_state: HashMap<u32,String>
}

pub struct Log
{

    last_exec_op: u32,
    // hashmap with operation sequence number associated with the operation itself.
    // TODO create an enum for the types of operation
    operations: HashMap<u32,Operation>, 
    checkpoint: Option<Checkpoint>,

}

impl Log {
    pub fn new() -> Self {
        Self { 
            last_exec_op: 0,
            operations: HashMap::new(), 
            checkpoint: None
        }
    }

    pub fn log_operation(&mut self,seqno: u32, operation: Operation) -> bool {
        match self.operations.insert(seqno, operation) {
            Some(_) => panic!("repeated sequence number"),
            None => true,
        }
    }
}