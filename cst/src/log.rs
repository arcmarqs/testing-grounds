use std::collections::HashMap;

/* 
Simplified log implementation to test state transfers 
*/
use crate::node::Operation;

#[derive(Debug)]
pub struct Checkpoint 
{
    checkpoint_seq: u32,
    last_exec_op: u32,
    app_state: HashMap<u32,String>
}

#[derive(Debug)]
pub struct LoggedOperation {
    seqno: u32, 
    operation: Operation
}

impl LoggedOperation {
    pub fn new(seqno: u32, operation: Operation) -> Self {
        Self {
            seqno,
            operation
        }
    }
}
#[derive(Debug)]
pub struct Log
{

    last_exec_op: u32,
    // hashmap with operation sequence number associated with the operation itself.
    // TODO create an enum for the types of operation
    operations: Vec<LoggedOperation>, 
    checkpoint: Option<Checkpoint>,

}

impl Log {
    pub fn new() -> Self {
        Self { 
            last_exec_op: 0,
            operations: Vec::new(), 
            checkpoint: None
        }
    }

    pub fn log_operation(&mut self,seqno: u32, operation: Operation) {
        self.operations.push(LoggedOperation::new(seqno, operation))
    }

    pub fn print_log(&self) {
        println!("{:?}", self.operations);
    }
}
