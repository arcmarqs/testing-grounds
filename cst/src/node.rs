/* Node structure simulating a replica running febft */

use std::collections::HashSet;
use std::ops::Add;
use crate::log::Log;

#[derive(Debug)]
pub enum NodeState {
    Idle,
    Running,
    Recovery
}

#[derive(Debug)]
pub enum Operation {
    Read(String),
    Insert(String),
    Remove(String),
}

#[derive(Debug)]
pub struct Node {
    state: NodeState,
    id: u32,
    log: Log,
    sequence_number: u32,
    data : HashSet<String>,
}

impl Node {
    pub fn new(id: u32) -> Self 
    {
        Self { 
            state: NodeState::Idle,
            id,
            log: Log::new(),
            sequence_number: 0,
            data: HashSet::new(),
        }
    }

    pub fn process_operation(&mut self,op: Operation) -> bool {
        let successful_op: bool;

        //Im only logging operations that change the state of the database
        match &op {
            Operation::Read(value) => {
                return self.data.contains(value) 
            }
            Operation::Insert(value) => {
                successful_op = self.data.insert(value.to_string())
            }
            Operation::Remove(value) => {
                successful_op = self.data.remove(value)
            }
        }

        match successful_op {
            true => {
                self.sequence_number += 1;
                self.log.log_operation(self.sequence_number,op);
                true

            }
            false =>  {
                false
            }
        }
    }

    pub fn print_data(&self) {
        println!("{:?}",self.data);
    }

    pub fn print_log(&self) {
        self.log.print_log();
    }
    
}